//! The GPU: a headless device (Metal on macOS) that vello renders a scene
//! into, read back as RGBA rows.

use std::sync::mpsc;

use anyhow::{Context, anyhow};
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

/// What a device must allow: wgpu's rows are copied out in multiples of this.
const ROW_ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The adapter's name and backend, as doctor reports them.
    pub name: String,
    pub backend: String,
    /// The widest and tallest picture it renders, in pixels.
    pub max_side: u32,
}

impl Gpu {
    /// The system's GPU, or why there is none.
    pub fn open() -> anyhow::Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
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
            device,
            queue,
            name: info.name,
            backend: format!("{:?}", info.backend),
            max_side: limits.max_texture_dimension_2d,
        })
    }

    /// `scene` rendered `width` × `height` pixels: RGBA, row after row.
    pub fn render(&self, scene: &Scene, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
        let mut renderer = Renderer::new(&self.device, RendererOptions::default())
            .map_err(|e| anyhow!("vello could not start: {e}"))?;
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("picture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
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
