//! The GPU (Metal on macOS) that vello renders a scene on: headless, the
//! picture read back as RGBA rows, or for a window, the picture presented
//! in it.

use std::sync::mpsc;

use anyhow::{Context, anyhow, bail};
use vello::wgpu;
use vello::wgpu::util::TextureBlitter;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

/// What a device must allow: wgpu's rows are copied out in multiples of this.
const ROW_ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

pub struct Gpu {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The adapter's name and backend, as doctor reports them.
    pub name: String,
    pub backend: String,
    /// The widest and tallest picture it renders, in pixels.
    pub max_side: u32,
    /// vello, started on the first picture and kept for the next: starting
    /// it builds its shaders, too slow to do for each frame of a window.
    renderer: Option<Renderer>,
}

/// A window's surface, set up for the GPU that presents in it.
pub struct Window<'w> {
    surface: wgpu::Surface<'w>,
    config: wgpu::SurfaceConfiguration,
    /// vello draws only into a storage texture, which a window's surface is
    /// not: its picture is copied across.
    blitter: TextureBlitter,
}

/// The entry to the system's GPUs; a window's display is not needed on any
/// platform underdraw runs on.
fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
}

impl Gpu {
    /// The system's GPU, or why there is none.
    pub fn open() -> anyhow::Result<Gpu> {
        Gpu::on(&instance(), None)
    }

    /// The GPU that can draw in `window`, `width` × `height` pixels, and the
    /// window's surface set up for it; or why there is none.
    pub fn open_for_window<'w>(
        window: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> anyhow::Result<(Gpu, Window<'w>)> {
        let instance = instance();
        let surface = instance.create_surface(window).context("the window gave the GPU no surface")?;
        let gpu = Gpu::on(&instance, Some(&surface))?;
        let format = surface
            .get_capabilities(&gpu.adapter)
            .formats
            .into_iter()
            .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm))
            .context("the window takes no 8-bit RGBA or BGRA pixels")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        let blitter = TextureBlitter::new(&gpu.device, format);
        Ok((gpu, Window { surface, config, blitter }))
    }

    /// The adapter that can draw on `surface`, when there is one, and a
    /// device on it.
    fn on(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> anyhow::Result<Gpu> {
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            ..Default::default()
        };
        let adapter = pollster::block_on(instance.request_adapter(&options))
            .context("no GPU adapter: rendering needs Metal (macOS), Vulkan or DirectX 12")?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("underdraw"),
            required_limits: limits.clone(),
            ..Default::default()
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&descriptor)).context("the GPU refused a device")?;
        Ok(Gpu {
            adapter,
            device,
            queue,
            name: info.name,
            backend: format!("{:?}", info.backend),
            max_side: limits.max_texture_dimension_2d,
            renderer: None,
        })
    }

    /// `scene` drawn `width` × `height` pixels into a texture of its own,
    /// which `usage` says what else may be done with.
    fn draw(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        usage: wgpu::TextureUsages,
    ) -> anyhow::Result<wgpu::Texture> {
        let mut renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(&self.device, RendererOptions::default())
                .map_err(|e| anyhow!("vello could not start: {e}"))?,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = RenderParams {
            base_color: vello::peniko::Color::WHITE,
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };
        renderer
            .render_to_texture(&self.device, &self.queue, scene, &view, &params)
            .map_err(|e| anyhow!("vello could not render: {e}"))?;
        self.renderer = Some(renderer);
        Ok(texture)
    }

    /// `scene` presented in `window`, filling it: the window's size is the
    /// one it was opened or last resized at.
    pub fn present(&mut self, window: &Window<'_>, scene: &Scene) -> anyhow::Result<()> {
        let frame = match window.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            }
            // Hidden, or not ready for a frame yet: the next one is drawn when it is.
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            other => bail!("the window's surface could not be drawn on: {other:?}"),
        };
        let (width, height) = (window.config.width, window.config.height);
        let picture = self.draw(scene, width, height, wgpu::TextureUsages::TEXTURE_BINDING)?;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        window.blitter.copy(
            &self.device,
            &mut encoder,
            &picture.create_view(&wgpu::TextureViewDescriptor::default()),
            &frame.texture.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }

    /// `window` set up again at `width` × `height` pixels, as when it is
    /// resized.
    pub fn resize(&self, window: &mut Window<'_>, width: u32, height: u32) {
        window.config.width = width;
        window.config.height = height;
        window.surface.configure(&self.device, &window.config);
    }

    /// `scene` rendered `width` × `height` pixels and read back: RGBA, row
    /// after row.
    pub fn render(&mut self, scene: &Scene, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
        let texture = self.draw(scene, width, height, wgpu::TextureUsages::COPY_SRC)?;
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let row = width * 4;
        let padded = row.div_ceil(ROW_ALIGN) * ROW_ALIGN;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        let (sent, mapped) = mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sent.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| anyhow!("the GPU did not finish: {e}"))?;
        mapped.recv()?.map_err(|e| anyhow!("the picture could not be read back: {e}"))?;
        let view =
            buffer.get_mapped_range(..).map_err(|e| anyhow!("the picture could not be read back: {e:?}"))?;
        let mut pixels = Vec::with_capacity((row * height) as usize);
        for line in view.chunks(padded as usize) {
            pixels.extend_from_slice(&line[..row as usize]);
        }
        Ok(pixels)
    }
}
