/**
 * Décodage d'une piste en PCM mono float 48 kHz via `ffmpeg` (doit être dans le PATH).
 *
 * ffmpeg décode et rééchantillonne (rééchantillonneur `swr` par défaut, réglages fixés ici pour ne pas dépendre de la
 * configuration locale) en gardant les canaux d'origine ; le passage en mono se fait ici, comme le mixage descendant
 * de l'`AnalyserNode` (« speakers » : stéréo → mono = (G + D) / 2, en float).
 */

export const SAMPLE_RATE = 48_000;

export interface DecodedTrack {
    pcm: Float32Array;
    channels: number;
    sourceRate: number;
}

export async function decodeMono48k(path: string): Promise<DecodedTrack> {
    const probe = await run(["ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels,sample_rate", "-of", "json", path]);
    const info = JSON.parse(new TextDecoder().decode(probe)) as { streams?: { channels?: number; sample_rate?: string }[] };
    const stream = info.streams?.[0];
    const channels = stream?.channels ?? 0;
    const sourceRate = Number(stream?.sample_rate ?? 0);
    if (channels !== 1 && channels !== 2) throw new Error(`${path} : ${channels} canaux, seuls mono et stéréo sont gérés`);

    const raw = await run([
        "ffmpeg", "-v", "error", "-nostdin", "-i", path,
        "-map", "0:a:0", "-af", `aresample=${SAMPLE_RATE}:resampler=swr`,
        "-ac", String(channels), "-f", "f32le", "-acodec", "pcm_f32le", "-",
    ]);
    if (raw.byteLength % (4 * channels) !== 0) throw new Error(`${path} : sortie ffmpeg tronquée`);
    const interleaved = new Float32Array(raw.buffer, raw.byteOffset, raw.byteLength / 4);
    if (channels === 1) return { pcm: interleaved.slice(), channels, sourceRate };

    const frames = interleaved.length / 2;
    const pcm = new Float32Array(frames);
    for (let i = 0; i < frames; i++) pcm[i] = 0.5 * (interleaved[2 * i]! + interleaved[2 * i + 1]!);
    return { pcm, channels, sourceRate };
}

async function run(cmd: string[]): Promise<Uint8Array> {
    const proc = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe" });
    const [out, err, code] = await Promise.all([
        new Response(proc.stdout).arrayBuffer(),
        new Response(proc.stderr).text(),
        proc.exited,
    ]);
    if (code !== 0) throw new Error(`${cmd[0]} a échoué (code ${code}) : ${err.trim()}`);
    return new Uint8Array(out);
}
