import init, {
    WasmSchematic as WebWasmSchematic,
    decodeLitematicMetadata as webDecodeLitematicMetadata,
} from './wasm/web/litemarus.js';
import type { ILitematicMetadata, ILoadedBlockState } from './adapter.js';

type WasmSchematicInstance = InstanceType<typeof WebWasmSchematic>;

let ready: Promise<void> | null = null;
let WasmSchematic: typeof WebWasmSchematic = WebWasmSchematic;
let decodeLitematicMetadata: typeof webDecodeLitematicMetadata = webDecodeLitematicMetadata;

async function ensureWasm(): Promise<void> {
    if (ready) {
        await ready;
        return;
    }
    if (typeof window === 'undefined') {
        ready = import('./node/register.js').then((mod) => {
            WasmSchematic = mod.WasmSchematic;
            decodeLitematicMetadata = mod.decodeLitematicMetadata;
        });
    } else {
        ready = init().then(() => undefined);
    }
    await ready;
}

/** 仅解析 .litematic 元数据（不展开全部方块） */
export async function decodeLitematicMetadataFromBytes(
    bytes: Uint8Array
): Promise<ILitematicMetadata> {
    await ensureWasm();
    return JSON.parse(decodeLitematicMetadata(bytes)) as ILitematicMetadata;
}

/** 已加载的 .litematic 读取器 */
export class LitematicReader {
    private _schematic: WasmSchematicInstance;

    private constructor(schematic: WasmSchematicInstance) {
        this._schematic = schematic;
    }

    public static async fromBytes(bytes: Uint8Array): Promise<LitematicReader> {
        await ensureWasm();
        return new LitematicReader(WasmSchematic.fromBytes(bytes));
    }

    public get metadata(): ILitematicMetadata {
        return {
            name: this._schematic.name(),
            author: this._schematic.author(),
            description: this._schematic.description(),
            width: this._schematic.width(),
            height: this._schematic.height(),
            length: this._schematic.length(),
            lmVersion: this._schematic.lm_version(),
            lmSubversion: this._schematic.lm_subversion(),
            mcVersion: this._schematic.mc_version(),
            created: Number(this._schematic.created()),
            modified: Number(this._schematic.modified()),
            regionNames: Array.from(this._schematic.regionNames() as unknown as string[]),
        };
    }

    /** 读取指定区域的方块列表（区域局部坐标，默认跳过空气） */
    public getRegionBlocks(regionKey: string, skipAir = true): ILoadedBlockState[] {
        const region = this._schematic.getRegion(regionKey);
        try {
            return JSON.parse(region.collectBlocksJson(skipAir)) as ILoadedBlockState[];
        } finally {
            region.free();
        }
    }

    /**
     * 合并所有区域的方块（schematic 坐标系，默认跳过空气）。
     * 多区域时会加上各区域 `Position` 偏移。
     */
    public getAllBlocks(skipAir = true): ILoadedBlockState[] {
        const all: ILoadedBlockState[] = [];
        for (const regionKey of this.metadata.regionNames) {
            const region = this._schematic.getRegion(regionKey);
            try {
                const blocks = JSON.parse(region.collectBlocksJson(skipAir)) as ILoadedBlockState[];
                const ox = region.position_x();
                const oy = region.position_y();
                const oz = region.position_z();
                for (const block of blocks) {
                    all.push({
                        x: block.x + ox,
                        y: block.y + oy,
                        z: block.z + oz,
                        id: block.id,
                        properties: block.properties,
                    });
                }
            } finally {
                region.free();
            }
        }
        return all;
    }

    public free(): void {
        this._schematic.free();
    }
}
