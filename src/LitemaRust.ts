import type {
    LitematicAdapter,
    IBlockBatch,
    IBlockState,
    ILitematicOptions,
} from './adapter.js';
import { WasmSchematic, WasmRegion } from './node/register.js';
import { toBlockBatch } from './batch.js';
import fs from 'node:fs';

const MC_DATA_VERSION = 2975;
const LITEMATIC_VERSION = 6;
const LITEMATIC_SUBVERSION = 1;

export class LitemaRust implements LitematicAdapter {
    private _schematic: WasmSchematic;
    private _region: WasmRegion;
    private _options: ILitematicOptions;

    public constructor(options: ILitematicOptions) {
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
        // 这里注意一下，顺序有问题
        this._region = new WasmRegion(0, 0, 0, L, H, W);

        this._options = options;
    }

    public setBlocks(blocks: IBlockState[]) {
        this.setBlockBatch(toBlockBatch(blocks));
    }

    /** 批量写入：整个作品只跨 wasm 一次 */
    public setBlockBatch(batch: IBlockBatch) {
        this._region.applyBlocks(batch.positions, batch.indices, batch.palette);
    }

    public async saveToFile(file: string): Promise<void> {
        this._schematic.insert_region('main', this._region);
        const bytes = this._schematic.save_bytes();
        fs.writeFileSync(file, Buffer.from(bytes));
    }
}
