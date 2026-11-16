//! Models with surfaces of their own (`ta_formats::surface`, beside their 3DOs: Supreme
//! Commander's units as TA Reborn makes them, and any mod's): their vertices' texture coordinates
//! and smooth normals as the surface gives them, their polygons painted with its materials, and
//! nothing of Reborn's kit, caps or rounding (they are whole already).
//!
//! A material's paint is three layers of an array of BC3 textures of [`SIDE`] by [`SIDE`] texels
//! (its colour; its specular and team colour; its normals), read the first time a model of its
//! is drawn and kept: a battle holds the paint of the units in it, not of every unit there is.
//! Where to find a material's layers is a table the shaders read (`surface_slots`); until they
//! are read, its faces are drawn in its colour on average. Without block-compressed textures on
//! the graphics card, the layers are decoded and held as plain colour instead.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;
use ta_formats::dds;
use ta_formats::model3do::{FIXED_ONE, Model3do};
use ta_formats::surface::{Material, Surface};

use super::{FAR_FROM_EDGES, ModelId, ModelInfo, ModelLibrary, ModelVertex, PALETTE_FACE, PieceInfo, f16_bits};
use crate::occlusion::Footprint;

/// The side of a material's layers.
pub const SIDE: u32 = 512;
/// A model with a lighter one for far off (its surface's `far`) is drawn as itself from this
/// many pixels across on screen.
pub const FAR_PIXELS: f32 = 48.0;
/// Their mip levels (512 down to 1).
const LEVELS: u32 = SIDE.ilog2() + 1;
/// A material's layers: colour, specular and team colour, normals.
pub const LAYERS: u32 = 3;
/// A table entry for a material whose layers are not read.
const NOT_READ: u32 = u32::MAX;
/// The most materials read in a frame (the rest the next).
const READ_PER_FRAME: usize = 6;

/// Reads a file of the game's (a texture a surface names), or none.
pub type Loader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

struct Entry {
    material: Material,
    /// Its place in the array (its layers from three times it), once read.
    group: Option<u32>,
    /// Reading it failed: drawn in its colour on average from then on.
    failed: bool,
}

/// The materials of the models with surfaces, and their paint as far as it is read.
pub struct SurfaceTextures {
    entries: Vec<Entry>,
    by_textures: HashMap<(String, String, String), u32>,
    loader: Option<Loader>,
    /// Materials drawn this frame and not read yet.
    wanted: Vec<u32>,
    compressed: bool,
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    /// Materials the array has room for, and has.
    capacity: u32,
    groups: u32,
    pub(super) table: wgpu::Buffer,
    table_slots: u32,
}

impl SurfaceTextures {
    pub fn new(device: &wgpu::Device) -> SurfaceTextures {
        let compressed = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let (texture, view) = Self::array(device, compressed, 1, 4, 1);
        let table = Self::make_table(device, 16);
        SurfaceTextures { entries: Vec::new(), by_textures: HashMap::new(), loader: None, wanted: Vec::new(), compressed, texture, view, capacity: 0, groups: 0, table, table_slots: 16 }
    }

    /// What reads the textures the materials name (the game's files).
    pub fn set_loader(&mut self, loader: Loader) {
        self.loader = Some(loader);
    }

    /// The slot of `material` (one for every material of the same three textures): what the
    /// vertices of its faces name.
    pub fn register(&mut self, material: &Material) -> u32 {
        let key = (material.albedo.to_ascii_lowercase(), material.specular.to_ascii_lowercase(), material.normals.to_ascii_lowercase());
        if let Some(&slot) = self.by_textures.get(&key) {
            return slot;
        }
        let slot = self.entries.len() as u32;
        self.entries.push(Entry { material: material.clone(), group: None, failed: false });
        self.by_textures.insert(key, slot);
        slot
    }

    /// Asks for `slot`'s paint (a model of its drawn this frame).
    pub(super) fn want(&mut self, slot: u32) {
        if let Some(e) = self.entries.get(slot as usize)
            && e.group.is_none()
            && !e.failed
            && !self.wanted.contains(&slot)
        {
            self.wanted.push(slot);
        }
    }

    /// Reads the paint asked for (some of it, a frame), making room for it; whether the bind group
    /// must be made again (the array or the table replaced).
    pub(super) fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let mut rebind = false;
        if self.entries.len() as u32 > self.table_slots {
            let slots = (self.entries.len() as u32).next_power_of_two();
            self.table = Self::make_table(device, slots);
            self.table_slots = slots;
            let table: Vec<u32> = (0..slots).map(|s| self.entries.get(s as usize).and_then(|e| e.group).unwrap_or(NOT_READ)).collect();
            queue.write_buffer(&self.table, 0, bytemuck::cast_slice(&table));
            rebind = true;
        }
        if self.wanted.is_empty() {
            return rebind;
        }
        let Some(loader) = self.loader.clone() else {
            self.wanted.clear();
            return rebind;
        };
        // (Those past the frame's few are asked for again by the next frame's models.)
        let wanted: Vec<u32> = std::mem::take(&mut self.wanted).into_iter().take(READ_PER_FRAME).collect();
        let most = device.limits().max_texture_array_layers / LAYERS;
        for slot in wanted {
            let entry = &self.entries[slot as usize];
            let paths = [entry.material.albedo.clone(), entry.material.specular.clone(), entry.material.normals.clone()];
            let mut levels: Vec<Vec<Vec<u8>>> = Vec::with_capacity(3);
            for path in &paths {
                match loader(path).map(|bytes| dds::to_bc3_levels(&bytes, SIDE)) {
                    Some(Ok(l)) if l.len() == LEVELS as usize => levels.push(l),
                    Some(Err(e)) => log::warn!("{path}: {e}"),
                    _ => log::warn!("{path}: not in the game's files"),
                }
            }
            if levels.len() != 3 {
                self.entries[slot as usize].failed = true;
                continue;
            }
            if self.groups == self.capacity {
                if self.capacity >= most {
                    log::warn!("surface paint: no room for more than {most} materials");
                    self.entries[slot as usize].failed = true;
                    continue;
                }
                self.grow(device, queue, (self.capacity * 2).clamp(8, most));
                rebind = true;
            }
            let group = self.groups;
            self.groups += 1;
            for (k, maps) in levels.iter().enumerate() {
                self.write_layer(queue, group * LAYERS + k as u32, maps);
            }
            self.entries[slot as usize].group = Some(group);
            queue.write_buffer(&self.table, slot as u64 * 4, bytemuck::bytes_of(&group));
        }
        rebind
    }

    /// The array made `capacity` materials large, what it held copied over.
    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, capacity: u32) {
        let (texture, view) = Self::array(device, self.compressed, capacity * LAYERS, SIDE, LEVELS);
        if self.groups > 0 {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("surface paint grown") });
            for level in 0..LEVELS {
                let side = (SIDE >> level).max(1);
                // (Compressed, a level is copied in whole blocks.)
                let side = if self.compressed { side.max(4) } else { side };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: level, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: level, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: self.groups * LAYERS },
                );
            }
            queue.submit([encoder.finish()]);
        }
        self.texture = texture;
        self.view = view;
        self.capacity = capacity;
        log::info!("surface paint: room for {capacity} materials ({} MB)", capacity as u64 * LAYERS as u64 * layer_bytes(self.compressed) / (1 << 20));
    }

    /// One layer's levels (BC3 blocks), written as the array holds them.
    fn write_layer(&self, queue: &wgpu::Queue, layer: u32, levels: &[Vec<u8>]) {
        for (level, data) in levels.iter().enumerate() {
            let side = (SIDE >> level).max(1);
            let blocks = side.div_ceil(4);
            let target = wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: level as u32, origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::All };
            if self.compressed {
                let extent = wgpu::Extent3d { width: blocks * 4, height: blocks * 4, depth_or_array_layers: 1 };
                queue.write_texture(target, data, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(blocks * 16), rows_per_image: Some(blocks) }, extent);
            } else {
                let rgba: Vec<u8> = dds::decode_bc3(data, side, side).into_iter().flatten().collect();
                let extent = wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 };
                queue.write_texture(target, &rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(side) }, extent);
            }
        }
    }

    fn array(device: &wgpu::Device, compressed: bool, layers: u32, side: u32, levels: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("surface paint"),
            size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: layers },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if compressed { wgpu::TextureFormat::Bc3RgbaUnorm } else { wgpu::TextureFormat::Rgba8Unorm },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        (texture, view)
    }

    fn make_table(device: &wgpu::Device, slots: u32) -> wgpu::Buffer {
        use crate::gpu::CheckedBufferInit;
        let table = vec![NOT_READ; slots as usize];
        device.create_buffer_checked(&wgpu::util::BufferInitDescriptor { label: Some("surface slots"), contents: bytemuck::cast_slice(&table), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST })
    }

    /// How many materials there are, and how many are read.
    pub fn counts(&self) -> (usize, u32) {
        (self.entries.len(), self.groups)
    }
}

/// A layer's bytes with its mip levels.
fn layer_bytes(compressed: bool) -> u64 {
    (0..LEVELS).map(|l| ((SIDE >> l).max(1) as u64).pow(2) * if compressed { 1 } else { 4 }).sum()
}

impl ModelLibrary {
    /// A model with a surface of its own, registered under `name`: each piece's vertices as the 3DO
    /// places them with the surface's texture coordinates and normals; a polygon naming one of
    /// its materials painted with it (`slots`: each material's, from
    /// [`SurfaceTextures::register`]), any other in its palette colour.
    pub fn add_surfaced(&mut self, name: &str, model: &Model3do, surface: &Surface, slots: &[u32], palette: &ta_formats::image::Palette) -> ModelId {
        let key = name.to_ascii_lowercase();
        if let Some(&id) = self.by_name.get(&key) {
            return id;
        }
        let fixed = |v: [i32; 3]| Vec3::new(v[0] as f32, v[1] as f32, -(v[2] as f32)) / FIXED_ONE;
        let base_vertex = self.vertices.len() as i32;
        let first_index = self.indices.len() as u32;
        let mut rest: Vec<Vec3> = Vec::with_capacity(model.pieces.len());
        for p in &model.pieces {
            let at = p.parent.map_or(Vec3::ZERO, |parent| rest[parent]) + fixed(p.offset);
            rest.push(at);
        }
        let (mut max_r2, mut max_h) = (0f32, 0f32);
        let mut polygons: Vec<Vec<Vec3>> = Vec::new();
        let mut ranges = Vec::with_capacity(model.pieces.len());
        for (pi, (piece, faces)) in model.pieces.iter().zip(&surface.pieces).enumerate() {
            let piece_first = self.indices.len() as u32;
            // (A vertex of the library's for each of the 3DO's and the slot it is painted with:
            // shared by the polygons round it, as its surface's normal is.)
            let mut made: HashMap<(u16, u32), u32> = HashMap::new();
            for prim in piece.primitives.iter().filter(|p| p.indices.len() >= 3) {
                let material = prim.texture.as_deref().and_then(|t| surface.material(t));
                let (layer, color) = match material {
                    Some(m) => {
                        let mean = surface.materials[m].mean;
                        (slots[m], [mean[0], mean[1], mean[2], 0])
                    }
                    None => {
                        let [r, g, b] = palette.rgb((prim.color_index & 0xFF) as u8);
                        (0xFFFF | (1 << 16) | PALETTE_FACE, [r, g, b, 255])
                    }
                };
                let mut corners: Vec<u32> = Vec::with_capacity(prim.indices.len());
                for &i in &prim.indices {
                    let p = fixed(piece.vertices[i as usize]);
                    let at = *made.entry((i, layer)).or_insert_with(|| {
                        let v = faces.vertices.get(i as usize).copied().unwrap_or(ta_formats::surface::Vertex { uv: [0.0; 2], normal: [0, 32767, 0] });
                        let n = Vec3::new(v.normal[0] as f32, v.normal[1] as f32, -(v.normal[2] as f32)).normalize_or(Vec3::Y);
                        self.vertices.push(ModelVertex {
                            pos: p.to_array(),
                            // (A middling roughness: the material's own is in its paint.)
                            normal: [(n.x * 127.0) as i8, (n.y * 127.0) as i8, (n.z * 127.0) as i8, 64],
                            uv: [(v.uv[0].clamp(0.0, 1.0) * 65535.0).round() as u16, (v.uv[1].clamp(0.0, 1.0) * 65535.0).round() as u16],
                            layer,
                            color,
                            piece: pi as u32,
                            edge_dist: [f16_bits(FAR_FROM_EDGES); 4],
                            edge_weight: [0; 4],
                            tread: [0.0; 2],
                        });
                        let world = rest[pi] + p;
                        max_r2 = max_r2.max(world.x * world.x + world.z * world.z);
                        max_h = max_h.max(world.y);
                        (self.vertices.len() as i32 - 1 - base_vertex) as u32
                    });
                    corners.push(at);
                }
                for k in 1..corners.len() - 1 {
                    self.indices.extend_from_slice(&[corners[0], corners[k + 1], corners[k]]);
                }
                polygons.push(prim.indices.iter().map(|&i| rest[pi] + fixed(piece.vertices[i as usize])).collect());
            }
            ranges.push((piece_first, self.indices.len() as u32 - piece_first));
        }
        let pieces = model
            .pieces
            .iter()
            .zip(&ranges)
            .map(|(p, &(first_index, index_count))| PieceInfo { name: p.name.clone(), parent: p.parent, offset: fixed(p.offset), first_index, index_count })
            .collect();
        let id = self.models.len() as ModelId;
        self.models.push(ModelInfo {
            name: name.to_string(),
            first_index,
            index_count: self.indices.len() as u32 - first_index,
            base_vertex,
            pieces,
            radius: max_r2.sqrt(),
            height: max_h,
            footprint: Footprint::of(&polygons, rest.first().copied().unwrap_or(Vec3::ZERO)),
        });
        self.by_name.insert(key, id);
        let mut used: Vec<u32> = slots.to_vec();
        used.sort_unstable();
        used.dedup();
        self.surfaced.insert(id, used);
        self.dirty = true;
        id
    }

    /// Draws `far` in `near`'s place far off (`FAR_PIXELS`): both models with surfaces, of the
    /// same pieces in the same order (or nothing is done; whether it was).
    pub fn set_far(&mut self, near: ModelId, far: ModelId) -> bool {
        let (a, b) = (&self.models[near as usize].pieces, &self.models[far as usize].pieces);
        let same = a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.name.eq_ignore_ascii_case(&y.name) && x.parent == y.parent);
        if same {
            self.far.insert(near, far);
        }
        same
    }

    /// The material slots a model with a surface paints with (none for any other).
    pub(super) fn surface_slots(&self, id: ModelId) -> &[u32] {
        self.surfaced.get(&id).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_formats::model3do::{Piece, Primitive};
    use ta_formats::surface;

    /// A hull and a turret on it, a triangle each: the turret's painted with the surface's
    /// material, the hull's in its palette colour (it names no material).
    #[test]
    fn a_surfaced_model_takes_its_surfaces_coordinates_normals_and_slots() {
        let tri = |texture: Option<&str>| Primitive { color_index: 7, indices: vec![0, 1, 2], texture: texture.map(str::to_string) };
        let corners = vec![[0, 0, 0], [65536, 0, 0], [0, 0, 65536]];
        let model = Model3do {
            pieces: vec![
                Piece { name: "base".into(), parent: None, offset: [0; 3], vertices: corners.clone(), primitives: vec![tri(None)], selection_primitive: None, children: vec![1] },
                Piece { name: "turret".into(), parent: Some(0), offset: [0, 4 << 16, 0], vertices: corners, primitives: vec![tri(Some("Paint"))], selection_primitive: None, children: vec![] },
            ],
        };
        let vertex = |u: f32| surface::Vertex { uv: [u, 0.5], normal: [0, 0, 32767] };
        let surface = Surface {
            materials: vec![Material { name: "paint".into(), albedo: "a.dds".into(), specular: "s.dds".into(), normals: "n.dds".into(), mean: [10, 20, 30] }],
            pieces: vec![surface::Piece { vertices: vec![vertex(0.0); 3] }, surface::Piece { vertices: vec![vertex(0.0), vertex(1.0), vertex(0.25)] }],
            far: String::new(),
        };
        let palette = ta_formats::image::Palette::parse(&vec![0u8; 1024]).unwrap();
        let mut library = ModelLibrary::default();
        let id = library.add_surfaced("tank", &model, &surface, &[5], &palette);
        assert_eq!(library.add_surfaced("TANK", &model, &surface, &[5], &palette), id, "registered once");
        let info = library.info(id).clone();
        assert_eq!(info.pieces.len(), 2);
        assert_eq!(info.index_count, 6);
        assert!((info.height - 4.0).abs() < 1e-5);
        assert_eq!(library.surface_slots(id), [5]);
        let turret = &library.vertices[3..6];
        assert_eq!(turret.iter().map(|v| v.uv[0]).collect::<Vec<_>>(), [0, 65535, 16384]);
        assert!(turret.iter().all(|v| v.layer == 5 && v.color[..3] == [10, 20, 30] && v.piece == 1));
        // (The normal's z the other way round, as the places'.)
        assert_eq!(turret[0].normal[2], -127);
        assert_eq!(library.vertices[0].layer & 0xFFFF, 0xFFFF, "the hull in its palette colour");
        // The triangles wound as the library winds TA's.
        assert_eq!(&library.indices[3..6], &[3, 5, 4]);
        // A second triangle on the turret's corners shares their vertices.
        let mut twice = model.clone();
        twice.pieces[1].primitives.push(tri(Some("paint")));
        let mut library = ModelLibrary::default();
        let near = library.add_surfaced("tank", &twice, &surface, &[5], &palette);
        assert_eq!(library.vertices.len(), 6);
        assert_eq!(library.indices.len(), 9);
        // A far model of the same pieces is drawn in its place far off; another's is not taken.
        let far = library.add_surfaced("tank_lod1", &model, &surface, &[5], &palette);
        assert!(library.set_far(near, far));
        let root = glam::Mat4::IDENTITY;
        assert_eq!(library.detail_for(near, root, None, true), near, "no camera: as itself");
        let mut camera = crate::camera::Camera::new(glam::Vec2::new(1920.0, 1080.0));
        camera.zoom = 0.2;
        assert_eq!(library.detail_for(near, root, Some(&camera), false), far, "small on screen");
        camera.zoom = 40.0;
        assert_eq!(library.detail_for(near, root, Some(&camera), false), near, "large on screen");
        let mut other = model.clone();
        other.pieces[1].name = "gun".into();
        let stranger = library.add_surfaced("other", &other, &surface, &[5], &palette);
        assert!(!library.set_far(near, stranger));
    }
}
