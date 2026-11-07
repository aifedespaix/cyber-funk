#!/usr/bin/env python3
"""Lecteur des journaux de session de cyber-funk (voir crates/cf-debuglog et .claude/skills/debug-logs).

Conçu pour qu'un agent trouve la cause d'un plantage sans charger le journal : chaque commande rend un résumé borné
(`--limit`, 40 lignes par défaut) et des lignes courtes. Bibliothèque standard seulement.

    python tools/debuglog/dbg.py crash            # que s'est-il passé à la fin de la dernière session ?
    python tools/debuglog/dbg.py summary          # vue d'ensemble : durée, compteurs, répartition des faits
    python tools/debuglog/dbg.py timeline --around 83500 --window 3000   # faits autour d'un instant de simulation (ms)
    python tools/debuglog/dbg.py find slot1/cue2  # toutes les occurrences d'un cue / d'une référence, avec durées
    python tools/debuglog/dbg.py spans            # durées par type de fait + faits jamais terminés
    python tools/debuglog/dbg.py check            # incohérences logiques
    python tools/debuglog/dbg.py sessions         # liste des sessions

Options communes : --root logs (dossier des sessions), --session NOM (défaut : la dernière), --limit N.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections import Counter, defaultdict
from pathlib import Path

STALE_HEARTBEAT_MS = 5000


# --------------------------------------------------------------------------- chargement


def find_root(arg: str | None) -> Path:
    candidates = [Path(arg)] if arg else [Path("logs"), Path(os.environ.get("APPDATA", "")) / "cyber-funk" / "sessions"]
    for c in candidates:
        if c.is_dir():
            return c
    sys.exit(f"dossier de journaux introuvable ({', '.join(map(str, candidates))}) ; utiliser --root")


def sessions(root: Path) -> list[Path]:
    return sorted(p for p in root.iterdir() if p.is_dir() and p.name.startswith("session-"))


def pick_session(root: Path, name: str | None) -> Path:
    if name:
        path = root / name
        if not path.is_dir():
            sys.exit(f"session {name} introuvable")
        return path
    all_sessions = sessions(root)
    if not all_sessions:
        sys.exit("aucune session dans " + str(root))
    return all_sessions[-1]


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def read_events(session: Path) -> list[dict]:
    events = []
    path = session / "events.jsonl"
    if not path.is_file():
        return events
    with path.open(encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                events.append({"ev": "?truncated", "lvl": "error", "raw": line[:120], "up": 0, "sim": 0, "seq": -1})
    return events


def pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name == "nt":
        import ctypes

        handle = ctypes.windll.kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            return False
        code = ctypes.c_ulong()
        ok = ctypes.windll.kernel32.GetExitCodeProcess(handle, ctypes.byref(code))
        ctypes.windll.kernel32.CloseHandle(handle)
        return bool(ok) and code.value == 259  # STILL_ACTIVE
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


# --------------------------------------------------------------------------- affichage


def fmt_ms(ms: float) -> str:
    ms = int(ms)
    return f"{ms // 60000:d}:{ms // 1000 % 60:02d}.{ms % 1000:03d}"


def line(e: dict) -> str:
    ph = {"start": "▶", "end": "■", "point": "·"}.get(e.get("ph", "point"), "·")
    lvl = {"error": "ERR", "warn": "WRN"}.get(e.get("lvl", ""), "   ")
    parts = [f"#{e.get('seq', '?'):<5}", f"sim {fmt_ms(e.get('sim', 0))}", f"up {fmt_ms(e.get('up', 0))}", lvl, ph, e.get("ev", "?")]
    if e.get("ref"):
        parts.append(str(e["ref"]))
    if e.get("label"):
        parts.append(f"« {str(e['label'])[:80]} »")
    d = e.get("d")
    if isinstance(d, dict):
        d = {k: v for k, v in d.items() if k != "backtrace"}
        if d:
            parts.append(json.dumps(d, ensure_ascii=False, separators=(",", ":"))[:140])
    return " ".join(parts)


def emit(lines: list[str], limit: int) -> None:
    if len(lines) > limit:
        head = limit // 2
        tail = limit - head
        skipped = len(lines) - limit
        lines = lines[:head] + [f"… {skipped} lignes omises (--limit {limit}) …"] + lines[-tail:]
    print("\n".join(lines))


# --------------------------------------------------------------------------- commandes


def heartbeat_age_ms(session: Path, meta: dict) -> float | None:
    """Âge du battement de cœur par rapport à la dernière écriture de l'événement (ms), sans horloge de la machine."""
    hb = session / "heartbeat.json"
    ev = session / "events.jsonl"
    if not hb.is_file() or not ev.is_file():
        return None
    return abs(ev.stat().st_mtime - hb.stat().st_mtime) * 1000.0


def verdict(session: Path, events: list[dict]) -> tuple[str, list[str]]:
    """Retourne (diagnostic en une phrase, indices)."""
    meta = read_json(session / "session.json")
    hints: list[str] = []
    panics = [e for e in events if e.get("ev") == "panic"]
    stalls = [e for e in events if e.get("ev") == "watchdog.stall"]
    recovered = [e for e in events if e.get("ev") == "watchdog.recovered"]
    errors = [e for e in events if e.get("lvl") == "error" and e.get("ev") not in ("panic", "watchdog.stall", "watchdog.restart")]
    gpu = [e for e in events if e.get("ev") == "log" and any(k in (str(e.get("ref", "")) + str(e.get("label", ""))).lower() for k in ("wgpu", "device", "surface", "lost", "gpu", "vulkan", "dx12"))]
    slow = [e for e in events if e.get("ev") == "perf.slow_frame"]
    alive = pid_alive(int(meta.get("pid", 0) or 0))
    closed = meta.get("status") == "closed"

    if slow:
        worst = max((e.get("d", {}).get("ms", 0) for e in slow), default=0)
        hints.append(f"{len(slow)} images lentes (> 100 ms), pire {worst} ms — carte graphique ou disque saturés ?")
    if gpu:
        hints.append("messages graphiques (wgpu/périphérique/surface) : " + "; ".join(str(e.get("label", ""))[:90] for e in gpu[-2:]))
    if errors:
        hints.append("dernière erreur : " + str(errors[-1].get("label", errors[-1].get("ev")))[:140])

    if panics:
        p = panics[-1]
        return f"PANIQUE à {p.get('ref')} : {p.get('label')}", hints
    if stalls and (not recovered or recovered[-1].get("seq", 0) < stalls[-1].get("seq", 0)):
        gap = stalls[-1].get("d", {}).get("gap_ms")
        state = "le processus est encore vivant (gel en cours)" if alive else "puis le processus est mort"
        return f"GEL de la boucle principale (aucune image depuis {gap} ms) {state}", hints
    if closed:
        return "fermeture propre — pas de plantage dans cette session", hints
    if alive:
        return "session en cours (le processus tourne)", hints
    last = events[-1] if events else {}
    return (
        "ARRÊT BRUTAL sans panique ni fermeture : processus tué, plantage natif (pilote graphique, violation mémoire) ou coupure. "
        f"Dernier fait : {last.get('ev')} à sim {fmt_ms(last.get('sim', 0))}",
        hints,
    )


def cmd_crash(args, session: Path, events: list[dict]) -> None:
    meta = read_json(session / "session.json")
    hb = read_json(session / "heartbeat.json")
    out = [f"session {session.name}  status={meta.get('status')}  pid={meta.get('pid')}  démarrée {meta.get('started')}"]
    text, hints = verdict(session, events)
    out.append("DIAGNOSTIC : " + text)
    out += ["  indice : " + h for h in hints]
    if hb:
        out.append(
            "dernier battement : sim {} · image {} · pause={} · fps={} · cue={} · audio={}".format(
                fmt_ms(hb.get("sim_ms", 0)), hb.get("frame"), hb.get("paused"), hb.get("fps"),
                (hb.get("cue") or {}).get("label") or (hb.get("cue") or {}).get("ref"),
                json.dumps(hb.get("audio"), separators=(",", ":")),
            )
        )
    panics = [e for e in events if e.get("ev") == "panic"]
    if panics:
        bt = (panics[-1].get("d") or {}).get("backtrace", "")
        frames = [l.strip() for l in bt.splitlines() if "cyber" in l or "cf_" in l or "cf-" in l][:8]
        out.append("trace (cadres du projet) :")
        out += ["    " + f for f in frames]
    interesting = [i for i, e in enumerate(events) if e.get("lvl") in ("error", "warn") or e.get("ev") in ("sim.reset", "sim.pause", "sim.resume")]
    anchor = interesting[-1] if interesting else len(events) - 1
    out.append(f"--- contexte autour du fait #{events[anchor].get('seq') if events else '?'} (10 avant, 5 après) ---")
    out += [line(e) for e in events[max(0, anchor - 10): anchor + 6]]
    emit(out, args.limit + 12)


def cmd_summary(args, session: Path, events: list[dict]) -> None:
    meta = read_json(session / "session.json")
    count = Counter(e.get("ev") for e in events)
    lvl = Counter(e.get("lvl") for e in events)
    out = [
        f"session {session.name}  status={meta.get('status')}  durée={fmt_ms(meta.get('duration_ms', events[-1].get('up', 0) if events else 0))}",
        f"{len(events)} faits  erreurs={lvl.get('error', 0)}  avertissements={lvl.get('warn', 0)}  args={meta.get('args')}",
        "faits par type : " + ", ".join(f"{k}×{v}" for k, v in count.most_common(14)),
    ]
    cues = [e for e in events if e.get("ev") == "cue.active" and e.get("ph") == "start"]
    out.append(f"{len(cues)} cues joués : " + " → ".join(str(e.get("ref")) for e in cues[:12]) + (" …" if len(cues) > 12 else ""))
    out.append("verdict : " + verdict(session, events)[0])
    emit(out, args.limit)


def matches(e: dict, args) -> bool:
    if args.ev and not str(e.get("ev", "")).startswith(args.ev):
        return False
    if args.ref and args.ref not in str(e.get("ref", "")) + str(e.get("label", "")):
        return False
    if args.level and e.get("lvl") not in args.level.split(","):
        return False
    return True


def cmd_timeline(args, session: Path, events: list[dict]) -> None:
    sel = events
    if args.around is not None:
        sel = [e for e in sel if abs(e.get("sim", 0) - args.around) <= args.window]
    if args.up_around is not None:
        sel = [e for e in sel if abs(e.get("up", 0) - args.up_around) <= args.window]
    sel = [e for e in sel if matches(e, args)]
    if args.skip_noise:
        sel = [e for e in sel if e.get("ev") not in ("action",)]
    emit([line(e) for e in sel] or ["(aucun fait)"], args.limit)


def pair_spans(events: list[dict]):
    open_spans: dict[int, dict] = {}
    done: list[tuple[dict, dict]] = []
    for e in events:
        sid = e.get("sid")
        if sid is None:
            continue
        if e.get("ph") == "start":
            open_spans[sid] = e
        elif e.get("ph") == "end" and sid in open_spans:
            done.append((open_spans.pop(sid), e))
    return done, list(open_spans.values())


def cmd_find(args, session: Path, events: list[dict]) -> None:
    needle = args.target.lower()
    hit = [e for e in events if needle in (str(e.get("ref", "")) + " " + str(e.get("label", ""))).lower()]
    done, unclosed = pair_spans(events)
    out = [f"{len(hit)} faits contiennent « {args.target} »"]
    refs = Counter((e.get("ref"), e.get("label")) for e in hit if e.get("ref"))
    out.append("références : " + "; ".join(f"{r} « {l} » ×{n}" for (r, l), n in refs.most_common(6)))
    durs = [(s, e) for s, e in done if needle in (str(s.get("ref", "")) + " " + str(s.get("label", ""))).lower()]
    for s, e in durs[:12]:
        out.append(f"  {s.get('ev')} {s.get('ref')} : sim {fmt_ms(s.get('sim', 0))} → {fmt_ms(e.get('sim', 0))}  ({(e.get('d') or {}).get('dur_ms')} ms réels, cause {(s.get('d') or {}).get('cause')})")
    out += [line(e) for e in hit[: args.limit]]
    emit(out, args.limit + 16)


def cmd_spans(args, session: Path, events: list[dict]) -> None:
    done, unclosed = pair_spans(events)
    by_ev: dict[str, list[float]] = defaultdict(list)
    for _, e in done:
        by_ev[e.get("ev")].append((e.get("d") or {}).get("dur_ms", 0))
    out = []
    for ev, durs in sorted(by_ev.items()):
        durs.sort()
        out.append(f"{ev}: n={len(durs)} min={durs[0]}ms médiane={durs[len(durs) // 2]}ms max={durs[-1]}ms")
    out.append(f"faits jamais terminés : {len(unclosed)}")
    out += ["  " + line(e) for e in unclosed[-8:]]
    emit(out, args.limit)


def cmd_check(args, session: Path, events: list[dict]) -> None:
    problems: list[str] = []
    last_sim = -1
    last_seq = 0
    paused = False
    pause_sim = 0
    active_cues: list[dict] = []
    for e in events:
        ev, ph = e.get("ev"), e.get("ph")
        seq = e.get("seq", 0)
        if seq and last_seq and seq != last_seq + 1 and seq > 0:
            problems.append(f"#{seq} trou dans la numérotation (précédent #{last_seq}) : lignes perdues ou fichier tronqué")
        last_seq = seq or last_seq
        sim = e.get("sim", 0)
        if sim + 50 < last_sim and last_sim > 0:
            # Normal juste après un reset.
            if not any(p.startswith("RESET") for p in problems[-1:]) and ev != "session.end":
                prior = [x for x in events if x.get("seq", 0) < seq][-3:]
                if not any(x.get("ev") == "sim.reset" for x in prior):
                    problems.append(f"#{seq} l'horloge de simulation recule ({fmt_ms(last_sim)} → {fmt_ms(sim)}) sans reset")
        last_sim = max(last_sim, sim) if ev != "sim.reset" else 0
        if ev == "sim.pause":
            paused, pause_sim = True, sim
        elif ev == "sim.resume":
            if not paused:
                problems.append(f"#{seq} reprise sans pause préalable")
            paused = False
        elif paused and sim > pause_sim + 100 and ev not in ("log", "perf.slow_frame", "watchdog.stall", "watchdog.recovered", "audio.underrun", "panic", "session.end"):
            problems.append(f"#{seq} {ev} à sim {fmt_ms(sim)} alors que la simulation est en pause depuis {fmt_ms(pause_sim)}")
            paused = False
        if ev == "cue.active":
            if ph == "start":
                if active_cues:
                    problems.append(f"#{seq} cue {e.get('ref')} démarre alors que {active_cues[-1].get('ref')} n'est pas terminé (deux cues actifs)")
                prev = (e.get("d") or {}).get("from")
                if prev and active_cues and active_cues[-1].get("ref") != prev:
                    problems.append(f"#{seq} l'entrée annonce venir de {prev} mais le cue actif était {active_cues[-1].get('ref')}")
                active_cues.append(e)
            elif ph == "end" and active_cues:
                active_cues.pop()
        if ev == "sim.reset":
            active_cues.clear()
    _, unclosed = pair_spans(events)
    stale = [e for e in unclosed if e.get("ev") != "cue.active"]
    for e in stale[:5]:
        problems.append(f"#{e.get('seq')} {e.get('ev')} {e.get('ref')} commencé mais jamais terminé")
    out = [f"{len(problems)} incohérence(s)"] + problems
    emit(out, args.limit)


def cmd_sessions(args, root: Path) -> None:
    out = []
    for s in reversed(sessions(root)[-args.limit:]):
        meta = read_json(s / "session.json")
        crashed = meta.get("status") == "running" and not pid_alive(int(meta.get("pid", 0) or 0))
        tag = "PLANTAGE?" if crashed else meta.get("status")
        out.append(f"{s.name:36} {tag:10} err={meta.get('errors', '?')} wrn={meta.get('warns', '?')} stalls={meta.get('stalls', '?')}")
    print("\n".join(out) or "(aucune session)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["crash", "summary", "timeline", "find", "spans", "check", "sessions"])
    parser.add_argument("target", nargs="?", help="pour find : référence ou nom de cue")
    parser.add_argument("--root")
    parser.add_argument("--session")
    parser.add_argument("--limit", type=int, default=40)
    parser.add_argument("--around", type=int, help="instant de simulation (ms)")
    parser.add_argument("--up-around", type=int, help="instant réel depuis le démarrage (ms)")
    parser.add_argument("--window", type=int, default=2000)
    parser.add_argument("--ev", help="préfixe du nom de fait")
    parser.add_argument("--ref")
    parser.add_argument("--level", help="ex. warn,error")
    parser.add_argument("--skip-noise", action="store_true")
    args = parser.parse_args()
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")

    root = find_root(args.root)
    if args.command == "sessions":
        cmd_sessions(args, root)
        return
    session = pick_session(root, args.session)
    events = read_events(session)
    if args.command == "find" and not args.target:
        sys.exit("find demande une référence (ex. slot1/cue2 ou un nom)")
    {"crash": cmd_crash, "summary": cmd_summary, "timeline": cmd_timeline, "find": cmd_find, "spans": cmd_spans, "check": cmd_check}[args.command](args, session, events)


if __name__ == "__main__":
    main()
