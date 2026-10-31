import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const assets = resolve(repo, 'assets');
/** Dossiers où chercher des modèles de voiture (GLB, ou FBX brut avant le travail dans Blender) (le dépôt est la source de vérité, rien n'est copié). */
const roots = ['assets/car-src', 'assets/models/car'].map((d) => resolve(repo, d));

function listGlb(dir: string, depth: number, out: { path: string; size: number; mtime: number }[]) {
  if (!existsSync(dir) || depth < 0) return;
  for (const name of readdirSync(dir)) {
    const full = resolve(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) listGlb(full, depth - 1, out);
    else if (/\.(glb|fbx)$/i.test(name))
      out.push({ path: relative(repo, full).split(sep).join('/'), size: st.size, mtime: st.mtimeMs });
  }
}

/** Sert les GLB du dépôt et prévient la page quand l'un d'eux change (export Blender, build-glb.sh…). */
function carModels(): Plugin {
  return {
    name: 'car-models',
    configureServer(server) {
      server.watcher.add(roots);
      server.middlewares.use('/api/models', (_req, res) => {
        const out: { path: string; size: number; mtime: number }[] = [];
        for (const r of roots) listGlb(r, 3, out);
        res.setHeader('content-type', 'application/json');
        res.end(JSON.stringify(out));
      });
      server.middlewares.use('/glb', (req, res) => {
        const rel = decodeURIComponent((req.url ?? '').split('?')[0] ?? '').replace(/^\/+/, '');
        let full = resolve(repo, rel);
        // Un FBX parle de `large_car_d.tif` (relatif à son dossier) ; les textures livrées sont dans ../textures/*.tga.png.
        if (!existsSync(full) && /\.(tif|tiff|tga|dds)$/i.test(full)) {
          const stem = (full.split(sep).pop() ?? '').split('.')[0] ?? '';
          const alt = resolve(dirname(full), '..', 'textures', `${stem}.tga.png`);
          if (existsSync(alt)) full = alt;
        }
        // Garde-fou : uniquement des modèles et textures sous assets/.
        if (!full.startsWith(assets + sep) || !/\.(glb|fbx|png|jpe?g|webp)$/i.test(full) || !existsSync(full)) {
          res.statusCode = 404;
          res.end('introuvable');
          return;
        }
        res.setHeader('content-type', full.toLowerCase().endsWith('.glb') ? 'model/gltf-binary' : 'application/octet-stream');
        res.setHeader('cache-control', 'no-store');
        createReadStream(full).pipe(res);
      });
      let timer: ReturnType<typeof setTimeout> | undefined;
      const changed = (file: string) => {
        if (!/\.(glb|fbx)$/i.test(file)) return;
        // Blender écrit en plusieurs fois : on attend que le fichier se stabilise.
        clearTimeout(timer);
        timer = setTimeout(() => {
          server.ws.send({ type: 'custom', event: 'glb-changed', data: { path: relative(repo, file).split(sep).join('/') } });
        }, 700);
      };
      server.watcher.on('change', changed);
      server.watcher.on('add', changed);
    },
  };
}

export default defineConfig({ plugins: [carModels()], server: { port: 5199, strictPort: false } });
