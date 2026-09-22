# litemarus

Rust 重写版 [litemapy](https://github.com/SmylerMC/litemapy)：一个用于读取与写入
Litematica 投影文件（`.litematic`）的库，同时支持原生 Rust 与 WebAssembly
（Node / 浏览器）。

`litemarus` 是 litemapy 的 **衍生作品（port）**，对外保持与原库相同的文件协议语义：
相同的输入得到等价的解析结果与写回结果，产出的 `.litematic` 文件可被 Litematica 正常加载。
处于性能考虑，某些接口形式和内部实现略有变化。

> **协议声明**：本仓库基于 [SmylerMC/litemapy](https://github.com/SmylerMC/litemapy)
> 本项目同样以 **GNU GPL v3.0（`GPL-3.0-only`）** 发布。

## 特性

- 完整读写 `.litematic` 文件（Gzip + NBT）
- 多区域（Region）、方块调色板（Palette）、实体（Entity）、方块实体（TileEntity）支持
- 计划刻（PendingBlockTicks / PendingFluidTicks）与预览图数据原样读写
- 稀疏方块存储：仅记录非空气方块，内存占用与「实际放置的方块数」而非外接体积成正比
- 流式写出：大尺寸 3D 地图画保存时不再需要整块 NBT 树，峰值内存显著下降
- 原生 `rlib` / `cdylib` 与 `wasm32-unknown-unknown` 双目标
- TypeScript 封装（Node 与浏览器），批量写入只跨 wasm 一次

## 安装

### Rust

发布到 crates.io 后：

```toml
[dependencies]
litemarus = "0.11"
```

当前可直接依赖 Git 仓库：

```toml
[dependencies]
litemarus = { git = "https://github.com/TgkRuobin/litemarus" }
```

### npm

```bash
npm install @mcpixelart/litemarus
```

## 快速开始

### Rust：创建并保存

```rust
use litemarus::minecraft::BlockState;
use litemarus::region::Region;
use litemarus::schematic::{SaveMeta, Schematic};

fn main() -> litemarus::Result<()> {
    // 在 (0,0,0) 处创建一个 2×2×2 的区域
    let mut region = Region::new(0, 0, 0, 2, 2, 2)?;
    let stone = BlockState::new("minecraft:stone")?;
    region.set_block_at(0, 0, 0, stone)?;

    let mut schematic = Schematic::with_regions(
        "planet",
        "litemarus",
        "Made with litemarus",
        vec![("main".into(), region)],
    );

    schematic.save("planet.litematic", SaveMeta::default())?;
    Ok(())
}
```

### Rust：读取

```rust
use litemarus::schematic::Schematic;

fn main() -> litemarus::Result<()> {
    let schematic = Schematic::load("planet.litematic")?;
    let region = schematic.regions().get("main").unwrap();
    let block = region.get_block_at(0, 0, 0);
    println!("block at (0,0,0): {}", block.id());
    Ok(())
}
```

### TypeScript / Node：写入

```ts
import { LitemaRust } from '@mcpixelart/litemarus';

const writer = new LitemaRust({
    name: 'planet',
    author: 'litemarus',
    description: 'Made with litemarus',
    width: 21,
    height: 21,
    length: 21,
    filename: 'planet.litematic',
});

writer.setBlocks([
    { name: 'minecraft:light_blue_concrete', pos: [10, 10, 10] },
]);

await writer.saveToFile('planet.litematic');
```

### TypeScript / Node：读取

```ts
import { readFileSync } from 'node:fs';
import {
	LitematicReader,
	decodeLitematicMetadataFromBytes,
} from '@mcpixelart/litemarus';

const bytes = new Uint8Array(readFileSync('planet.litematic'));

// 仅解析元数据，不展开全部方块
const meta = await decodeLitematicMetadataFromBytes(bytes);
console.log(meta.name, meta.author);

// 完整读取
const reader = await LitematicReader.fromBytes(bytes);
console.log(reader.metadata);

// 读取区域方块（区域局部坐标，默认跳过空气）
const blocks = reader.getRegionBlocks('main');
reader.free();
```

## 构建

### 原生

```bash
cargo build
cargo test
```

### WASM + TypeScript

需要安装 [wasm-pack](https://rustwasm.github.io/wasm-pack/) 与 Rust
`wasm32-unknown-unknown` 目标：

```bash
rustup target add wasm32-unknown-unknown
npm install
npm run compile:rust   # wasm-pack build + 拷贝产物到 src/wasm
npm run build          # tsc 编译 TypeScript + 拷贝 wasm 到 dist
```

## API 概览

核心类型：

- `Schematic`：整张投影图，含元数据与多区域管理。
- `Region`：子区域，含方块调色板、实体、方块实体与计划刻。
- `BlockState`：方块状态（资源标识符 + 属性）。
- `Entity` / `TileEntity`：实体与方块实体。
- `LitematicaBitArray` / `DiscriminatingDictionary`：底层存储结构。

## 许可证

本仓库是 [litemapy](https://github.com/SmylerMC/litemapy) 的衍生作品，采用
**GNU General Public License v3.0（`GPL-3.0-only`）** 许可，许可证全文见
[LICENSE](LICENSE)。

版权声明：

- 原始项目 litemapy © [SmylerMC](https://github.com/SmylerMC)，GPL-3.0。
- 本重写 litemarus © 2025-2026 [TgkRuobin](https://github.com/TgkRuobin)。

本程序是自由软件，您可以在 GNU GPL v3 条款下重新分发和/或修改它；本程序不提供任何
担保，包括但不限于适销性或特定用途适用性的默示担保。完整的许可证文本见
[LICENSE](LICENSE)，或访问 <https://www.gnu.org/licenses/>。

## 致谢

- [litemapy](https://github.com/SmylerMC/litemapy)：本项目的源库，感谢 SmylerMC。
- [Litematica](https://github.com/maruohon/litematica)：Minecraft 投影模组，由 maruohon 开发。
