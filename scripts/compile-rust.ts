import { execSync } from 'child_process';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CRATE_DIR = path.join(__dirname, '..');

const WEB_CMD = 'wasm-pack build --target web --out-dir pkg/web --features wasm';
const SRC_DIR = path.join(CRATE_DIR, 'pkg/web');
const DIST_DIRS = [
    path.join(CRATE_DIR, 'src/wasm/web'),
    path.join(CRATE_DIR, 'src/wasm/node'),
];

function buildAndMove() {
    try {
        console.log('\n🚀 Building litemarus WASM (web target, used for node + browser)...');
        execSync(WEB_CMD, { cwd: CRATE_DIR, stdio: 'inherit' });

        const files = fs.readdirSync(SRC_DIR).filter(
            (f) => f !== 'package.json' && f !== '.gitignore'
        );

        for (const distDir of DIST_DIRS) {
            fs.mkdirSync(distDir, { recursive: true });
            for (const file of files) {
                fs.copyFileSync(path.join(SRC_DIR, file), path.join(distDir, file));
                console.log(`✅ ${file} -> ${path.basename(distDir)}`);
            }
        }

        console.log('\n✨ WASM build complete.');
    } catch (error) {
        console.error('\n❌ Build failed:', error);
        process.exit(1);
    }
}

buildAndMove();
