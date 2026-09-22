/**
 * 投影构造信息
 */
export interface ILitematicOptions {
    /** z */
    width: number;
    /** y */
    height: number;
    /** x */
    length: number;
    /** 作者 */
    author: string;
    /** 作品名称 */
    name: string;
    /** 作品描述 */
    description: string;
    /** 文件保存地址 */
    filename: string;
}

/** 方块及其 NBT 数据 */
export interface IBlockState {
    name: string;
    pos: [number, number, number];
    properties?: { [key: string]: string };
}

/** 方块状态（不含坐标），批量写入时的调色板条目 */
export interface IBlockStateSpec {
    name: string;
    properties?: { [key: string]: string };
}

/**
 * 批量写入载荷：跨 wasm 只传三个参数。
 *
 * `positions` 为扁平的 `[x0,y0,z0, x1,y1,z1, ...]`，`indices[i]` 指向 `palette[indices[i]]`。
 * 调色板去重后通常只有几十~几百项，所以每个方块状态只需要解析一次。
 */
export interface IBlockBatch {
    positions: Int32Array;
    indices: Uint32Array;
    palette: IBlockStateSpec[];
}

/** 从 .litematic 解析出的元数据 */
export interface ILitematicMetadata {
    name: string;
    author: string;
    description: string;
    width: number;
    height: number;
    length: number;
    lmVersion: number;
    lmSubversion: number;
    mcVersion: number;
    created: number;
    modified: number;
    regionNames: string[];
}

/** 从 .litematic 区域读出的方块（区域局部坐标） */
export interface ILoadedBlockState {
    x: number;
    y: number;
    z: number;
    id: string;
    properties: Record<string, string>;
}

export abstract class LitematicAdapter {
    public constructor(_options: ILitematicOptions) {}

    public abstract setBlocks(blocks: IBlockState[]): void;

    public abstract setBlockBatch(batch: IBlockBatch): void;

    public abstract saveToFile(file: string): Promise<void>;
}
