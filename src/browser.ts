export type {
    ILitematicOptions,
    IBlockState,
    IBlockStateSpec,
    IBlockBatch,
    ILitematicMetadata,
    ILoadedBlockState,
    LitematicAdapter,
} from './adapter.js';
export { LitematicReader, decodeLitematicMetadataFromBytes } from './reader.js';
export { toBlockBatch } from './batch.js';

import init, { WasmSchematic, WasmRegion } from './wasm/web/litemarus.js';
import type {
    LitematicAdapter,
    IBlockBatch,
    IBlockState,
    ILitematicOptions,
} from './adapter.js';
import { toBlockBatch } from './batch.js';

const MC_DATA_VERSION = 2975;
const LITEMATIC_VERSION = 6;
const LITEMATIC_SUBVERSION = 1;

let ready: ReturnType<typeof init> | null = null;

async function ensureWasm() {
    if (!ready) {
        ready = init();
    }
    await ready;
}

export class LitemaRust implements LitematicAdapter {
    private _schematic!: WasmSchematic;
    private _region!: WasmRegion;
    private _options: ILitematicOptions;
    private _initPromise: Promise<void>;

    public constructor(options: ILitematicOptions) {
        this._options = options;
        this._initPromise = ensureWasm().then(() => {
            this._schematic = new WasmSchematic(
                options.name,
                options.author,
                options.description,
                MC_DATA_VERSION
            );
            this._schematic.set_lm_version(LITEMATIC_VERSION);
            this._schematic.set_lm_subversion(LITEMATIC_SUBVERSION);

            const W = options.width;
            const L = options.length;
            const H = options.height;
            this._region = new WasmRegion(0, 0, 0, L, H, W);
        });
    }

    public async setBlocks(blocks: IBlockState[]): Promise<void> {
        await this.setBlockBatch(toBlockBatch(blocks));
    }

    /** 批量写入：整个作品只跨 wasm 一次 */
    public async setBlockBatch(batch: IBlockBatch): Promise<void> {
        await this._initPromise;
        this._region.applyBlocks(batch.positions, batch.indices, batch.palette);
    }

    public async toBytes(): Promise<Uint8Array> {
        await this._initPromise;
        this._schematic.insert_region('main', this._region);
        return new Uint8Array(this._schematic.save_bytes());
    }

    public async saveToFile(file: string): Promise<void> {
        const bytes = await this.toBytes();
        const blob = new Blob([new Uint8Array(bytes)]);
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = this._options.filename;
        a.click();
        URL.revokeObjectURL(url);
        void file;
    }
}

export default LitemaRust;
