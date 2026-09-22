import fs from 'node:fs';
import { initSync, WasmSchematic, WasmRegion, decodeLitematicMetadata } from '../wasm/node/litemarus.js';

const wasmBuffer = fs.readFileSync(new URL('../wasm/node/litemarus_bg.wasm', import.meta.url));
initSync({ module: wasmBuffer });

export { WasmSchematic, WasmRegion, decodeLitematicMetadata };
