use litemarus::minecraft::BlockState;
use litemarus::nbt_io::write_nbt_uncompressed;
use litemarus::region::{Region, AIR};
use litemarus::schematic::{SaveMeta, Schematic};
use std::collections::HashSet;
use tempfile::NamedTempFile;

#[test]
fn roundtrip_small_schematic() {
    let mut reg = Region::new(0, 0, 0, 2, 2, 2).unwrap();
    let stone = BlockState::new("minecraft:stone").unwrap();
    reg.set_block_at(0, 0, 0, stone.clone()).unwrap();
    let mut sch = Schematic::with_regions("test", "author", "desc", vec![("r0".into(), reg)]);
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_owned();
    sch.save(&path, SaveMeta::default()).unwrap();
    let loaded = Schematic::load(&path).unwrap();
    assert_eq!(loaded.name(), "test");
    let r = loaded.regions().get("r0").unwrap();
    assert_eq!(r.get_block_at(0, 0, 0), &stone);
    assert_eq!(r.get_block_at(1, 1, 1), &*AIR);
}

#[test]
fn region_min_max_matches_litemapy_tests() {
    let reg = Region::new(0, 0, 0, 10, 20, 30).unwrap();
    assert_eq!(reg.min_schem_x(), 0);
    assert_eq!(reg.max_schem_x(), 9);
    assert_eq!(reg.min_x(), 0);
    assert_eq!(reg.max_x(), 9);
}

/// A region filled with a pseudo-random selection of `palette_size` block states
/// (plus air), which drives the packed bit width through every interesting value.
fn sample_region(seed: u64, palette_size: usize, width: i32, height: i32, length: i32) -> Region {
    let mut region = Region::new(0, 0, 0, width, height, length).unwrap();
    let blocks: Vec<BlockState> = (0..palette_size)
        .map(|i| BlockState::new(&format!("minecraft:codex_block_{i}")).unwrap())
        .collect();
    let mut state = seed | 1;
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let pick = ((state >> 33) as usize) % (palette_size + 2);
                if pick < palette_size {
                    region.set_block_at(x, y, z, blocks[pick].clone()).unwrap();
                }
            }
        }
    }
    region
}

fn sample_schematic(palette_size: usize) -> Schematic {
    let mut sch = Schematic::with_regions(
        "streaming",
        "author",
        "desc",
        vec![(
            "main".into(),
            sample_region(7 + palette_size as u64, palette_size, 6, 4, 5),
        )],
    );
    sch.regions_mut()
        .insert("offset".into(), sample_region(99, 3, 3, 3, 3));
    sch.set_preview(vec![1, 2, 3, 4]);
    // Fixed timestamps so two builds of the same input are byte-identical.
    sch.set_created(1_700_000_000_000);
    sch.set_modified(1_700_000_000_000);
    sch
}

/// The streaming writer must be byte-identical to the tree writer it replaced.
#[test]
fn streaming_writer_matches_tree_writer() {
    for palette_size in [1usize, 2, 4, 5, 8, 16, 17, 32, 33, 100] {
        let mut tree_side = sample_schematic(palette_size);
        let tree_bytes = {
            let nbt = tree_side.to_nbt(true).unwrap();
            write_nbt_uncompressed(&nbt).unwrap()
        };

        let mut stream_side = sample_schematic(palette_size);
        let mut stream_bytes = Vec::new();
        stream_side.write_nbt_to(&mut stream_bytes, true).unwrap();

        assert_eq!(
            tree_bytes, stream_bytes,
            "palette size {palette_size}: streaming NBT differs from the tree writer"
        );
    }
}

/// Same check for the gzip output of `save_bytes`.
#[test]
fn save_bytes_matches_legacy_gzip_output() {
    let options = SaveMeta {
        update_meta: false,
        ..SaveMeta::default()
    };
    for palette_size in [1usize, 5, 33] {
        let mut tree_side = sample_schematic(palette_size);
        let legacy = litemarus::nbt_io::write_gzip_nbt(
            &tree_side.to_nbt(true).unwrap(),
            options.gzip_compression,
        )
        .unwrap();

        let mut stream_side = sample_schematic(palette_size);
        let streamed = stream_side.save_bytes(options.clone()).unwrap();

        assert_eq!(legacy, streamed, "palette size {palette_size}");
    }
}

/// The reported bug: a large 3D map art box (mostly air) must save without ever
/// materialising the whole bounding box.
#[test]
fn large_sparse_region_round_trips() {
    let (width, height, length) = (800i32, 100i32, 800i32); // 64M cells, ~0.15% filled
    let stone = BlockState::new("minecraft:stone").unwrap();
    let mut region = Region::new(0, 0, 0, width, height, length).unwrap();

    let mut placed: HashSet<(i32, i32, i32)> = HashSet::new();
    for i in 0..100_000usize {
        let x = (i * 7 % width as usize) as i32;
        let y = (i % height as usize) as i32;
        let z = (i * 13 % length as usize) as i32;
        region.set_block_at(x, y, z, stone.clone()).unwrap();
        placed.insert((x, y, z));
    }
    assert_eq!(region.count_blocks(), placed.len());

    let mut sch = Schematic::with_regions("big", "author", "sparse", vec![("main".into(), region)]);
    let bytes = sch
        .save_bytes(SaveMeta {
            update_meta: false,
            ..SaveMeta::default()
        })
        .unwrap();

    let loaded = Schematic::from_bytes(&bytes).unwrap();
    let loaded_region = loaded.regions().get("main").unwrap();
    assert_eq!(loaded_region.count_blocks(), placed.len());
    assert_eq!(loaded_region.volume(), 64_000_000);
    for (x, y, z) in placed.iter().take(50) {
        assert_eq!(loaded_region.get_block_at(*x, *y, *z), &stone);
    }
    assert_eq!(loaded_region.get_block_at(1, 1, 1), &*AIR);
}

/// Bulk palette rewrites must keep working after the storage change.
#[test]
fn bulk_palette_operations_preserve_semantics() {
    let mut region = Region::new(0, 0, 0, 4, 1, 4).unwrap();
    let stone = BlockState::new("minecraft:stone").unwrap();
    region.set_block_at(0, 0, 0, stone.clone()).unwrap();
    assert_eq!(region.count_blocks(), 1);

    // air -> stone turns every implicit empty cell into an explicit block
    region.replace_block(&AIR, BlockState::new("minecraft:dirt").unwrap());
    assert_eq!(region.count_blocks(), 16);
    assert_eq!(
        region.get_block_at(3, 0, 3).id(),
        "minecraft:dirt",
        "cells that used to be air must take the replacement block"
    );
    assert_eq!(region.get_block_at(0, 0, 0), &stone);

    region.filter(|block| block.with_id("minecraft:gold_block").unwrap());
    assert_eq!(region.count_blocks(), 16);
    assert_eq!(region.get_block_at(1, 0, 1).id(), "minecraft:gold_block");
}

/// The batched write path (`intern_block_state` + `set_palette_index_at`) must be
/// indistinguishable from placing blocks one by one.
#[test]
fn palette_index_writes_match_block_state_writes() {
    let (width, height, length) = (8, 4, 8);
    let mut direct = Region::new(0, 0, 0, width, height, length).unwrap();
    let mut batched = Region::new(0, 0, 0, width, height, length).unwrap();

    let states: Vec<BlockState> = (0..5)
        .map(|i| BlockState::new(&format!("minecraft:codex_block_{i}")).unwrap())
        .collect();
    let slots: Vec<u32> = states
        .iter()
        .map(|state| batched.intern_block_state(state.clone()).unwrap())
        .collect();

    let mut rng = 12_345u64;
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                rng = rng
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let pick = ((rng >> 33) as usize) % (states.len() + 1);
                if pick < states.len() {
                    direct
                        .set_block_at(x, y, z, states[pick].clone())
                        .unwrap();
                    batched.set_palette_index_at(x, y, z, slots[pick]).unwrap();
                }
            }
        }
    }

    assert_eq!(direct.count_blocks(), batched.count_blocks());
    for x in 0..width {
        for y in 0..height {
            for z in 0..length {
                assert_eq!(direct.get_block_at(x, y, z), batched.get_block_at(x, y, z));
            }
        }
    }

    // Bad palette slots and out-of-bounds positions are rejected, not written.
    assert!(batched.set_palette_index_at(0, 0, 0, 99).is_err());
    assert!(batched.set_palette_index_at(99, 0, 0, slots[0]).is_err());
    assert!(direct.set_block_at(99, 0, 0, states[0].clone()).is_err());
    assert_eq!(direct.get_block_at(99, 0, 0), &*AIR);
}
