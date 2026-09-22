import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, '..');
const srcWasm = path.join(root, 'src/wasm');
const distWasm = path.join(root, 'dist/wasm');

function copyDir(from: string, to: string) {
    fs.mkdirSync(to, { recursive: true });
    for (const name of fs.readdirSync(from)) {
        const src = path.join(from, name);
        const dest = path.join(to, name);
        if (fs.statSync(src).isDirectory()) {
            copyDir(src, dest);
        } else {
            fs.copyFileSync(src, dest);
        }
    }
}

if (!fs.existsSync(srcWasm)) {
    console.error('Missing src/wasm — run pnpm compile:rust first.');
    process.exit(1);
}

copyDir(srcWasm, distWasm);
console.log('✅ Copied wasm artifacts to dist/wasm');
