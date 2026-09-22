import type { IBlockBatch, IBlockState, IBlockStateSpec } from './adapter.js';

const NO_PROPERTIES = '\u0000';

function paletteKey(block: IBlockStateSpec): string {
    if (!block.properties) {
        return block.name + NO_PROPERTIES;
    }
    // Property maps are tiny and few (they belong to distinct states), so a
    // stable string is cheaper than comparing objects per block.
    const keys = Object.keys(block.properties).sort();
    let key = block.name + NO_PROPERTIES;
    for (const name of keys) {
        key += `${name}=${block.properties[name]};`;
    }
    return key;
}

/**
 * `IBlockState[]` → 批量载荷。
 *
 * 去重后只保留一份方块状态表，坐标与索引写入 TypedArray，
 * 跨 wasm 调用从「每方块一次」降到「整体一次」。
 */
export function toBlockBatch(blocks: IBlockState[]): IBlockBatch {
    const palette: IBlockStateSpec[] = [];
    const slotByKey = new Map<string, number>();
    const positions = new Int32Array(blocks.length * 3);
    const indices = new Uint32Array(blocks.length);

    for (let i = 0; i < blocks.length; i++) {
        const block = blocks[i];
        const key = paletteKey(block);
        let slot = slotByKey.get(key);
        if (slot === undefined) {
            slot = palette.length;
            palette.push({ name: block.name, properties: block.properties });
            slotByKey.set(key, slot);
        }
        indices[i] = slot;
        const base = i * 3;
        positions[base] = block.pos[0];
        positions[base + 1] = block.pos[1];
        positions[base + 2] = block.pos[2];
    }

    return { positions, indices, palette };
}
