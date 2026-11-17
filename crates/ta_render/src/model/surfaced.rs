//! Models with surfaces of their own (`ta_formats::surface`, beside their 3DOs: Supreme
//! Commander's units as TA Reborn makes them, and any mod's): their vertices' texture coordinates
//! and smooth normals as the surface gives them, their polygons painted with its materials, and
//! nothing of Reborn's kit, caps or rounding (they are whole already).
//!
//! A material's paint is three layers of an array of BC3 textures of [`SIDE`] by [`SIDE`] texels
//! (its colour; its specular and team colour; its normals), asked for the first time a model of
//! its is drawn: a battle holds the paint of the units in it, not of every unit there is, and lets
//! it go when the next battle starts ([`SurfaceTextures::release`]). Where to find a material's
//! layers is a table the shaders read (`surface_slots`); until they are in the array, its faces
//! are drawn in its colour on average. The files are read and made BC3 levels on workers of their
//! own (a texture of another size, or uncompressed, is decoded and encoded again, which takes tens
//! of milliseconds); the frame puts what they made in the array for [`FRAME_SHARE`] of its time
//! at most. The array grows [`GROWTH`] materials at a time; when the card has no memory for more,
//! the materials not in it stay in their colours on average. Without block-compressed textures on
//! the graphics card, the layers are decoded and held as plain colour instead.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use glam::Vec3;
use ta_formats::dds;
use ta_formats::model3do::{FIXED_ONE, Model3do};
use ta_formats::surface::{Material, Surface};

use super::{FAR_FROM_EDGES, ModelId, ModelInfo, ModelLibrary, ModelVertex, PALETTE_FACE, PieceInfo, TextureRef, f16_bits, face_normal, polygon_uvs, unflagged_ground_plate};
use crate::occlusion::Footprint;

/// The side of a material's layers.
pub const SIDE: u32 = 512;
/// A model with a lighter one for far off (its surface's `far`) is drawn as itself from this
/// many pixels across on screen, and as its lighter one under [`FAR_PIXELS_BACK`]: between the
/// two it stays as it was drawn the frame before (first seen there, the lighter one), so a model
/// small on the screen does not swap back and forth as the camera shakes or a walker bobs.
pub const FAR_PIXELS: f32 = 48.0;
pub const FAR_PIXELS_BACK: f32 = 42.0;
/// [`ModelLibrary::far_role`]s: a model with a lighter one for far off, and such a lighter one.
pub(super) const FAR_HAS: u8 = 1;
pub(super) const FAR_IS: u8 = 2;
/// Their mip levels (512 down to 1).
const LEVELS: u32 = SIDE.ilog2() + 1;
/// A material's layers: colour, specular and team colour, normals.
pub const LAYERS: u32 = 3;
/// A table entry for a material whose layers are not read.
const NOT_READ: u32 = u32::MAX;
/// The most of a frame's time spent putting the paint the workers made in the array (at least one
/// material a frame; the rest the next).
pub const FRAME_SHARE: Duration = Duration::from_millis(2);
/// How many materials the array grows by (about 32 MB of BC3).
pub const GROWTH: u32 = 32;
/// Materials made and waiting for the frames, on the way from the workers and taken from them
/// (twice this all told) at most: the workers wait for room (a material's levels are about a
/// megabyte, four as plain colour).
const WAITING: usize = 16;

/// Reads a file of the game's (a texture a surface names), or none.
pub type Loader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// Whether each frame waits for the paint it asks for ([`set_paint_waits`]).
static WAITS: AtomicBool = AtomicBool::new(false);
/// The longest a frame waits for a material's paint ([`set_paint_waits`]).
const WAIT_AT_MOST: Duration = Duration::from_secs(10);

/// With `waits`, each frame waits for the paint its models ask for, and puts all of it in the
/// array whatever it takes: a capture (`--screenshot`, a film) shows a unit in its paint from the
/// first frame it is in, whatever the workers' pace. Otherwise the paint comes as it is made.
pub fn set_paint_waits(waits: bool) {
    WAITS.store(waits, Ordering::Relaxed);
}

struct Entry {
    material: Material,
    /// Its place in the array (its layers from three times it), once read.
    group: Option<u32>,
    /// Asked of the workers (its paint on the way).
    asked: bool,
    /// Reading it failed, or the array had no room for it: drawn in its colour on average from
    /// then on (until the paint is let go).
    failed: bool,
}

/// A material's paint to read and make: its three textures' paths.
struct Job {
    slot: u32,
    generation: u32,
    paths: [String; 3],
    loader: Loader,
}

/// A material's paint as the workers made it: each layer's levels, as the array holds them (BC3
/// blocks, or plain colour), or why it could not be made.
struct Made {
    slot: u32,
    generation: u32,
    layers: Result<Vec<Vec<Vec<u8>>>, String>,
}

/// The workers reading and making paint, and the way to them and back.
struct Workers {
    jobs: mpsc::Sender<Job>,
    made: mpsc::Receiver<Made>,
}

/// The materials of the models with surfaces, and their paint as far as it is read.
pub struct SurfaceTextures {
    entries: Vec<Entry>,
    by_textures: HashMap<(String, String, String), u32>,
    loader: Option<Loader>,
    workers: Option<Workers>,
    /// What the workers made, waiting for room in a frame.
    ready: VecDeque<Made>,
    /// The paint's generation: what was asked before it was last let go is not wanted.
    generation: Arc<AtomicU32>,
    compressed: bool,
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    /// Materials the array has room for, and has.
    capacity: u32,
    groups: u32,
    /// The most materials the array may hold (the card's limit on its layers).
    most: u32,
    /// The card refused the memory to grow it: no more materials go in until the paint is let go.
    full: bool,
    pub(super) table: wgpu::Buffer,
    table_slots: u32,
    /// The bind group must be made again (the array or the table replaced since the last frame).
    rebind: bool,
}

impl SurfaceTextures {
    pub fn new(device: &wgpu::Device) -> SurfaceTextures {
        let compressed = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let (texture, view) = Self::array(device, compressed, 1, 4, 1);
        let table = Self::make_table(device, 16);
        let most = device.limits().max_texture_array_layers / LAYERS;
        SurfaceTextures {
            entries: Vec::new(),
            by_textures: HashMap::new(),
            loader: None,
            workers: None,
            ready: VecDeque::new(),
            generation: Arc::new(AtomicU32::new(0)),
            compressed,
            texture,
            view,
            capacity: 0,
            groups: 0,
            most,
            full: false,
            table,
            table_slots: 16,
            rebind: false,
        }
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
        self.entries.push(Entry { material: material.clone(), group: None, asked: false, failed: false });
        self.by_textures.insert(key, slot);
        slot
    }

    /// Asks for `slot`'s paint (a model of its drawn this frame): the workers read and make it,
    /// once.
    pub(super) fn want(&mut self, slot: u32) {
        let Some(e) = self.entries.get_mut(slot as usize) else { return };
        if e.asked || e.failed || e.group.is_some() {
            return;
        }
        let Some(loader) = self.loader.clone() else { return };
        if self.full {
            e.failed = true;
            return;
        }
        e.asked = true;
        let paths = [e.material.albedo.clone(), e.material.specular.clone(), e.material.normals.clone()];
        let job = Job { slot, generation: self.generation.load(Ordering::Relaxed), paths, loader };
        let compressed = self.compressed;
        let generation = &self.generation;
        let workers = self.workers.get_or_insert_with(|| Self::start_workers(compressed, generation.clone()));
        let _ = workers.jobs.send(job);
    }

    /// The workers: a few threads of their own (a texture to decode and encode again keeps one
    /// busy for tens of milliseconds), reading and making what they are asked in turn.
    fn start_workers(compressed: bool, generation: Arc<AtomicU32>) -> Workers {
        let (jobs, asked) = mpsc::channel::<Job>();
        let (done, made) = mpsc::sync_channel::<Made>(WAITING);
        let asked = Arc::new(Mutex::new(asked));
        let count = std::thread::available_parallelism().map_or(1, |n| (n.get() / 4).clamp(1, 3));
        for k in 0..count {
            let (asked, done, generation) = (asked.clone(), done.clone(), generation.clone());
            let spawned = std::thread::Builder::new().name(format!("surface paint {k}")).spawn(move || {
                loop {
                    let job = match asked.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    let Ok(job) = job else { return };
                    // (Paint let go since it was asked: not wanted.)
                    if job.generation != generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let layers = make_paint(&job.loader, &job.paths, compressed);
                    if done.send(Made { slot: job.slot, generation: job.generation, layers }).is_err() {
                        return;
                    }
                }
            });
            if let Err(e) = spawned {
                log::warn!("surface paint: no worker ({e})");
            }
        }
        Workers { jobs, made }
    }

    /// Puts the paint the workers made in the array, for [`FRAME_SHARE`] of the frame's time at
    /// most, making room for it; whether the bind group must be made again (the array or the table
    /// replaced).
    pub(super) fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let mut rebind = std::mem::take(&mut self.rebind);
        if self.entries.len() as u32 > self.table_slots {
            let slots = (self.entries.len() as u32).next_power_of_two();
            self.table = Self::make_table(device, slots);
            self.table_slots = slots;
            let table: Vec<u32> = (0..slots).map(|s| self.entries.get(s as usize).and_then(|e| e.group).unwrap_or(NOT_READ)).collect();
            queue.write_buffer(&self.table, 0, bytemuck::cast_slice(&table));
            rebind = true;
        }
        let generation = self.generation.load(Ordering::Relaxed);
        let waits = WAITS.load(Ordering::Relaxed);
        if let Some(workers) = &self.workers {
            // (What the workers made, as much as there is room for here: a few materials, the
            // workers waiting meanwhile; all of it when the frame waits for it.)
            while waits || self.ready.len() < WAITING {
                match workers.made.try_recv() {
                    Ok(m) => self.ready.push_back(m),
                    Err(_) => break,
                }
            }
            // (Waiting: for every material asked for and not made yet.)
            while waits {
                let made = self.ready.iter().filter(|m| m.generation == generation).count();
                let on_the_way = self.entries.iter().filter(|e| e.asked && e.group.is_none() && !e.failed).count();
                if made >= on_the_way {
                    break;
                }
                match workers.made.recv_timeout(WAIT_AT_MOST) {
                    Ok(m) => self.ready.push_back(m),
                    Err(_) => break,
                }
            }
        }
        let started = Instant::now();
        while let Some(made) = self.ready.pop_front() {
            if made.generation != generation {
                continue;
            }
            let slot = made.slot as usize;
            let layers = match made.layers {
                Ok(layers) => layers,
                Err(e) => {
                    log::warn!("{e}");
                    self.entries[slot].failed = true;
                    continue;
                }
            };
            if self.groups == self.capacity && !self.full {
                if self.capacity >= self.most {
                    log::warn!("surface paint: no room for more than {} materials; the rest are drawn in their colours on average", self.most);
                    self.full = true;
                } else if self.grow(device, queue, (self.capacity + GROWTH).min(self.most)) {
                    rebind = true;
                } else {
                    self.full = true;
                }
            }
            if self.groups == self.capacity {
                self.entries[slot].failed = true;
                continue;
            }
            let group = self.groups;
            self.groups += 1;
            for (k, maps) in layers.iter().enumerate() {
                self.write_layer(queue, group * LAYERS + k as u32, maps);
            }
            self.entries[slot].group = Some(group);
            queue.write_buffer(&self.table, slot as u64 * 4, bytemuck::bytes_of(&group));
            if !waits && started.elapsed() >= FRAME_SHARE {
                break;
            }
        }
        rebind
    }

    /// Lets the paint go (a battle over, the next starting): the array back to nothing, every
    /// material drawn in its colour on average until a model of its is drawn again; what the
    /// workers are making now is not wanted.
    pub fn release(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.ready.clear();
        for e in &mut self.entries {
            e.group = None;
            e.asked = false;
            e.failed = false;
        }
        let held = self.capacity;
        let (texture, view) = Self::array(device, self.compressed, 1, 4, 1);
        self.texture = texture;
        self.view = view;
        self.capacity = 0;
        self.groups = 0;
        self.full = false;
        queue.write_buffer(&self.table, 0, bytemuck::cast_slice(&vec![NOT_READ; self.table_slots as usize]));
        self.rebind = true;
        if held > 0 {
            log::info!("surface paint: let go of room for {held} materials");
        }
    }

    /// The array made `capacity` materials large, what it held copied over; false when the card
    /// refused the memory (the array as it was).
    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, capacity: u32) -> bool {
        let scopes = [wgpu::ErrorFilter::OutOfMemory, wgpu::ErrorFilter::Validation, wgpu::ErrorFilter::Internal].map(|f| device.push_error_scope(f));
        let (texture, view) = Self::array(device, self.compressed, capacity * LAYERS, SIDE, LEVELS);
        // (Popped innermost first, every one; the cause told, the outermost: a texture refused
        // for want of memory is then invalid to view as well.)
        let errors: Vec<wgpu::Error> = scopes.into_iter().rev().filter_map(|s| pollster::block_on(s.pop())).collect();
        if let Some(e) = errors.last() {
            log::warn!("surface paint: no graphics memory for room for {capacity} materials ({e}); the rest are drawn in their colours on average");
            return false;
        }
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
        true
    }

    /// One layer's levels (BC3 blocks, or plain colour), written as the array holds them.
    fn write_layer(&self, queue: &wgpu::Queue, layer: u32, levels: &[Vec<u8>]) {
        for (level, data) in levels.iter().enumerate() {
            let side = (SIDE >> level).max(1);
            let blocks = side.div_ceil(4);
            let target = wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: level as u32, origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::All };
            if self.compressed {
                let extent = wgpu::Extent3d { width: blocks * 4, height: blocks * 4, depth_or_array_layers: 1 };
                queue.write_texture(target, data, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(blocks * 16), rows_per_image: Some(blocks) }, extent);
            } else {
                let extent = wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 };
                queue.write_texture(target, data, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(side) }, extent);
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

    /// Whether every material asked for is in the array or given up (none on the way).
    pub fn settled(&self) -> bool {
        self.ready.is_empty() && self.entries.iter().all(|e| !e.asked || e.group.is_some() || e.failed)
    }
}

/// A material's paint read and made as the array holds it (on a worker): each of its three
/// textures (`paths`, read by `loader`) as BC3 levels of [`SIDE`] (`dds::to_bc3_levels`), decoded
/// to plain colour where the card reads no BC3 (`compressed` false); or why not, in words for the
/// log.
fn make_paint(loader: &Loader, paths: &[String; 3], compressed: bool) -> Result<Vec<Vec<Vec<u8>>>, String> {
    let mut layers = Vec::with_capacity(3);
    for path in paths {
        let bytes = loader(path).ok_or_else(|| format!("{path}: not in the game's files"))?;
        let levels = dds::to_bc3_levels(&bytes, SIDE).map_err(|e| format!("{path}: {e}"))?;
        if levels.len() != LEVELS as usize {
            return Err(format!("{path}: {} mip levels made of {LEVELS}", levels.len()));
        }
        layers.push(if compressed {
            levels
        } else {
            levels.iter().enumerate().map(|(level, data)| {
                let side = (SIDE >> level).max(1);
                dds::decode_bc3(data, side, side).into_iter().flatten().collect()
            }).collect()
        });
    }
    Ok(layers)
}

/// A layer's bytes with its mip levels.
fn layer_bytes(compressed: bool) -> u64 {
    (0..LEVELS).map(|l| ((SIDE >> l).max(1) as u64).pow(2) * if compressed { 1 } else { 4 }).sum()
}

impl ModelLibrary {
    /// A model with a surface of its own, registered under `name`: each piece's vertices as the 3DO
    /// places them with the surface's texture coordinates and normals; a polygon naming one of
    /// its materials painted with it (`slots`: each material's, from
    /// [`SurfaceTextures::register`]), one naming a TA texture (`texture` finds it) painted as TA
    /// lays it on, any other in its palette colour. The selection plate is left out, as from
    /// TA's own models.
    pub fn add_surfaced(&mut self, name: &str, model: &Model3do, surface: &Surface, slots: &[u32], texture: impl Fn(&str) -> Option<TextureRef>, palette: &ta_formats::image::Palette) -> ModelId {
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
            // (A vertex of the library's for each of the 3DO's and what paints it: shared by the
            // polygons round it, as its surface's normal is; a TA texture's laid on each polygon
            // of its own.)
            let mut made: HashMap<(u16, u32, [u8; 4]), u32> = HashMap::new();
            let plate = piece.selection_primitive.or_else(|| unflagged_ground_plate(piece, rest[pi].y));
            for (_, prim) in piece.primitives.iter().enumerate().filter(|&(k, p)| p.indices.len() >= 3 && Some(k) != plate) {
                let at_corners: Vec<Vec3> = prim.indices.iter().map(|&i| fixed(piece.vertices[i as usize])).collect();
                let material = prim.texture.as_deref().and_then(|t| surface.material(t));
                let ta = if material.is_none() { prim.texture.as_deref().and_then(&texture) } else { None };
                let laid = ta.map(|_| polygon_uvs(&at_corners, face_normal(&at_corners)));
                let (layer, color, roughness) = match (material, ta) {
                    (Some(m), _) => {
                        let mean = surface.materials[m].mean;
                        // (A middling roughness: the material's own is in its paint.)
                        (slots[m], [mean[0], mean[1], mean[2], 0], 64)
                    }
                    (None, Some(t)) => (t.layer | (t.frames.min(0xFF) << 16) | ((t.emissive as u32) << 24) | (1 << 28), [255, 255, 255, (t.metallic * 255.0).round() as u8], (t.roughness * 127.0).round() as i8),
                    (None, None) => {
                        let [r, g, b] = palette.rgb((prim.color_index & 0xFF) as u8);
                        (0xFFFF | (1 << 16) | PALETTE_FACE, [r, g, b, 255], 64)
                    }
                };
                let mut corners: Vec<u32> = Vec::with_capacity(prim.indices.len());
                for (c, (&i, &p)) in prim.indices.iter().zip(&at_corners).enumerate() {
                    let mut vertex = || {
                        let v = faces.vertices.get(i as usize).copied().unwrap_or(ta_formats::surface::Vertex { uv: [0.0; 2], normal: [0, 32767, 0] });
                        let n = Vec3::new(v.normal[0] as f32, v.normal[1] as f32, -(v.normal[2] as f32)).normalize_or(Vec3::Y);
                        let uv = laid.as_ref().map_or(v.uv, |uvs| uvs[c]);
                        self.vertices.push(ModelVertex {
                            pos: p.to_array(),
                            normal: [(n.x * 127.0) as i8, (n.y * 127.0) as i8, (n.z * 127.0) as i8, roughness],
                            uv: [(uv[0].clamp(0.0, 1.0) * 65535.0).round() as u16, (uv[1].clamp(0.0, 1.0) * 65535.0).round() as u16],
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
                    };
                    let at = if laid.is_some() { vertex() } else { *made.entry((i, layer, color)).or_insert_with(vertex) };
                    corners.push(at);
                }
                for k in 1..corners.len() - 1 {
                    self.indices.extend_from_slice(&[corners[0], corners[k + 1], corners[k]]);
                }
                polygons.push(at_corners.iter().map(|&p| rest[pi] + p).collect());
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
            let most = near.max(far) as usize + 1;
            if self.far_roles.len() < most {
                self.far_roles.resize(most, 0);
            }
            self.far_roles[near as usize] = FAR_HAS;
            self.far_roles[far as usize] = FAR_IS;
        }
        same
    }

    /// Whether a model has a lighter one drawn in its place far off ([`FAR_HAS`]), is such a
    /// lighter one ([`FAR_IS`]), or neither (0).
    pub(super) fn far_role(&self, id: ModelId) -> u8 {
        self.far_roles.get(id as usize).copied().unwrap_or(0)
    }

    /// The material slots a model with a surface paints with (none for any other).
    pub(super) fn surface_slots(&self, id: ModelId) -> &[u32] {
        self.surfaced.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Whether a model is painted with a surface of its own (`add_surfaced`).
    pub fn is_surfaced(&self, id: ModelId) -> bool {
        self.surfaced.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_formats::model3do::{Piece, Primitive};
    use ta_formats::surface;

    /// A DXT5 of 512 x 512 with all its levels (read as it is), as bytes.
    fn whole_paint() -> Vec<u8> {
        let mut file = dds::header(SIDE, SIDE, LEVELS, Some(b"DXT5"));
        file.extend(std::iter::repeat_n(0u8, (0..LEVELS).map(|l| ((SIDE >> l).max(1).div_ceil(4).pow(2) * 16) as usize).sum()));
        file
    }

    /// An uncompressed texture of 64 x 64 (decoded, grown and encoded again).
    fn small_paint() -> Vec<u8> {
        let mut file = dds::header(64, 64, 1, None);
        file.extend(std::iter::repeat_n(90u8, 64 * 64 * 4));
        file
    }

    fn loader() -> Loader {
        let files: HashMap<String, Vec<u8>> = [("whole.dds".to_string(), whole_paint()), ("small.dds".to_string(), small_paint())].into();
        Arc::new(move |path: &str| files.get(path).cloned())
    }

    fn material(albedo: &str, specular: &str, normals: &str) -> Material {
        Material { name: "paint".into(), albedo: albedo.into(), specular: specular.into(), normals: normals.into(), mean: [10, 20, 30] }
    }

    /// What the workers make of a material: its three textures as the array holds them, every
    /// level (BC3 blocks; plain colour for a card that reads none), or why not.
    #[test]
    fn paint_is_made_as_the_array_holds_it() {
        let paths = ["whole.dds".to_string(), "small.dds".to_string(), "whole.dds".to_string()];
        let made = make_paint(&loader(), &paths, true).unwrap();
        assert_eq!(made.len(), 3);
        for layer in &made {
            assert_eq!(layer.iter().map(Vec::len).collect::<Vec<_>>(), (0..LEVELS).map(|l| ((SIDE >> l).max(1).div_ceil(4).pow(2) * 16) as usize).collect::<Vec<_>>());
        }
        let plain = make_paint(&loader(), &paths, false).unwrap();
        assert_eq!(plain[1].iter().map(Vec::len).collect::<Vec<_>>(), (0..LEVELS).map(|l| ((SIDE >> l).max(1).pow(2) * 4) as usize).collect::<Vec<_>>());
        let missing = make_paint(&loader(), &["whole.dds".into(), "gone.dds".into(), "whole.dds".into()], true).unwrap_err();
        assert!(missing.contains("gone.dds"), "{missing}");
    }

    /// A material's paint is made on the workers and put in the array within the frame's share of
    /// time, room made a step at a time; one whose textures are not there stays in its colour on
    /// average; the paint is let go between battles and asked for again; and the card refusing
    /// the room leaves the array as it was.
    #[test]
    #[ignore = "requires a GPU; run with --ignored"]
    fn paint_is_made_on_workers_and_let_go() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("GPU adapter");
        let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: features, ..Default::default() })).expect("GPU device");
        let mut paint = SurfaceTextures::new(&device);
        let a = paint.register(&material("whole.dds", "small.dds", "whole.dds"));
        let b = paint.register(&material("whole.dds", "gone.dds", "whole.dds"));
        assert_eq!(paint.register(&material("WHOLE.dds", "small.dds", "whole.dds")), a, "one slot for the same textures");
        paint.want(a);
        assert!(paint.workers.is_none(), "nothing read without the game's files");
        paint.set_loader(loader());
        let settle = |paint: &mut SurfaceTextures| {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !paint.settled() {
                paint.update(&device, &queue);
                assert!(Instant::now() < deadline, "the paint was never made");
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        paint.want(a);
        paint.want(b);
        settle(&mut paint);
        assert_eq!(paint.counts(), (2, 1));
        assert_eq!((paint.entries[a as usize].group, paint.entries[b as usize].failed), (Some(0), true));
        assert_eq!(paint.capacity, GROWTH, "room for a step of materials");
        // Asked again: read once.
        paint.want(a);
        assert!(paint.settled());
        // Let go: nothing held, the bind group made again, read again when drawn.
        paint.release(&device, &queue);
        assert_eq!((paint.capacity, paint.groups, paint.entries[b as usize].failed), (0, 0, false));
        assert!(paint.update(&device, &queue), "the bind group is made again");
        paint.want(a);
        settle(&mut paint);
        assert_eq!(paint.entries[a as usize].group, Some(0));
        // Room the card cannot give (more layers than it has): refused, the array as it was.
        assert!(!paint.grow(&device, &queue, device.limits().max_texture_array_layers));
        assert_eq!(paint.capacity, GROWTH);
        // Room for one material at most: what is asked for past it is drawn in its colours on
        // average.
        paint.release(&device, &queue);
        paint.most = 1;
        paint.want(a);
        settle(&mut paint);
        assert_eq!((paint.capacity, paint.entries[a as usize].group), (1, Some(0)));
        let c = paint.register(&material("small.dds", "small.dds", "small.dds"));
        paint.want(c);
        settle(&mut paint);
        assert!(paint.entries[c as usize].failed && paint.full);
    }

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
        let none = |_: &str| None;
        let mut library = ModelLibrary::default();
        let id = library.add_surfaced("tank", &model, &surface, &[5], none, &palette);
        assert_eq!(library.add_surfaced("TANK", &model, &surface, &[5], none, &palette), id, "registered once");
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
        let near = library.add_surfaced("tank", &twice, &surface, &[5], none, &palette);
        assert_eq!(library.vertices.len(), 6);
        assert_eq!(library.indices.len(), 9);
        // A far model of the same pieces is drawn in its place far off; another's is not taken.
        let far = library.add_surfaced("tank_lod1", &model, &surface, &[5], none, &palette);
        assert!(library.set_far(near, far));
        let root = glam::Mat4::IDENTITY;
        assert_eq!(library.detail_for(near, root, None, true, None), near, "no camera: as itself");
        let mut camera = crate::camera::Camera::new(glam::Vec2::new(1920.0, 1080.0));
        camera.zoom = 0.2;
        assert_eq!(library.detail_for(near, root, Some(&camera), false, None), far, "small on screen");
        camera.zoom = 40.0;
        assert_eq!(library.detail_for(near, root, Some(&camera), false, None), near, "large on screen");
        assert_eq!((library.far_role(near), library.far_role(far), library.far_role(near + 7)), (FAR_HAS, FAR_IS, 0));
        let mut other = model.clone();
        other.pieces[1].name = "gun".into();
        let stranger = library.add_surfaced("other", &other, &surface, &[5], none, &palette);
        assert!(!library.set_far(near, stranger));
        // Between the two thresholds (45 pixels across, the classic camera's 2 x radius x zoom),
        // as it was drawn the frame before: as itself when it was, else its lighter one.
        let info = library.info(near).clone();
        camera.mode = crate::camera::CameraMode::Classic;
        camera.zoom = 45.0 / (2.0 * info.radius.max(info.height * 0.5));
        assert_eq!(library.detail_for(near, root, Some(&camera), false, Some(false)), near, "drawn as itself: it stays so");
        assert_eq!(library.detail_for(near, root, Some(&camera), false, Some(true)), far, "drawn far off: it stays so");
        assert_eq!(library.detail_for(near, root, Some(&camera), false, None), far, "first seen: under 48, far off");
        camera.zoom = 40.0 / (2.0 * info.radius.max(info.height * 0.5));
        assert_eq!(library.detail_for(near, root, Some(&camera), false, Some(false)), far, "under 42: far off");
        camera.zoom = 50.0 / (2.0 * info.radius.max(info.height * 0.5));
        assert_eq!(library.detail_for(near, root, Some(&camera), false, Some(true)), near, "over 48: as itself");
    }

    /// Polygons naming no material: one naming a TA texture painted with it, laid on as TA lays
    /// it (its own corners, its frames' count kept for team colours), palette faces of two
    /// colours not sharing their corners, and the selection plate left out.
    #[test]
    fn a_surfaced_models_other_polygons_are_drawn_as_tas() {
        let prim = |colour: u32, texture: Option<&str>| Primitive { color_index: colour, indices: vec![0, 1, 2], texture: texture.map(str::to_string) };
        let model = Model3do {
            pieces: vec![Piece {
                name: "base".into(),
                parent: None,
                offset: [0; 3],
                vertices: vec![[0, 0, 0], [65536, 0, 0], [0, 0, 65536]],
                primitives: vec![prim(0, Some("armcolour")), prim(1, None), prim(2, None), prim(3, None)],
                selection_primitive: Some(3),
                children: vec![],
            }],
        };
        let surface = Surface { materials: vec![], pieces: vec![surface::Piece { vertices: vec![surface::Vertex { uv: [0.0; 2], normal: [0, 32767, 0] }; 3] }], far: String::new() };
        let mut colours = vec![0u8; 1024];
        colours[4..8].copy_from_slice(&[200, 0, 0, 0]);
        colours[8..12].copy_from_slice(&[0, 200, 0, 0]);
        let palette = ta_formats::image::Palette::parse(&colours).unwrap();
        let team = TextureRef { layer: 7, frames: 10, emissive: false, roughness: 0.5, metallic: 0.2, mean: [128; 3] };
        let mut library = ModelLibrary::default();
        let id = library.add_surfaced("mixed", &model, &surface, &[], |n: &str| n.eq_ignore_ascii_case("armcolour").then_some(team), &palette);
        // Three polygons drawn (the plate left out), three corners each, none shared.
        assert_eq!(library.info(id).index_count, 9);
        assert_eq!(library.vertices.len(), 9);
        let painted = &library.vertices[0..3];
        assert!(painted.iter().all(|v| v.layer & 0xFFFF == 7 && (v.layer >> 16) & 0xFF == 10), "TA's texture, its ten frames");
        assert_ne!(painted[0].uv, painted[1].uv, "laid on as TA lays it");
        assert_eq!(library.vertices[3].color[..3], [200, 0, 0]);
        assert_eq!(library.vertices[6].color[..3], [0, 200, 0]);
    }
}
