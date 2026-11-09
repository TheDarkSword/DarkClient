//! GPU side of the Vulkan overlay: a render pass that loads the swapchain
//! image Minecraft just finished, egui drawn on top, and the synchronization
//! that slots our work between Minecraft's last submit and its present.

use crate::graphic::ui_engine::EguiFrame;
use anyhow::{anyhow, Context};
use ash::vk;
use egui_ash_renderer::{Options, Renderer};

/// Overlay frames that may be in flight at once. Each owns a command buffer
/// and a fence, and lines up with one of the renderer's vertex/index buffer
/// sets — both advance once per submitted draw.
const SLOTS: usize = 3;

/// How long to wait for one of our own frames before giving up on the
/// overlay rather than stalling the game.
const FENCE_TIMEOUT_NS: u64 = 1_000_000_000;

/// The game's Vulkan objects the overlay builds on. All are borrowed: the
/// overlay never destroys them.
pub struct Handles {
    pub get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub instance: vk::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: vk::Device,
    /// Family of the queue Minecraft presents on — its graphics queue.
    pub queue_family: u32,
    /// Format of the swapchain images.
    pub format: vk::Format,
}

struct Slot {
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
}

/// Per-swapchain objects, rebuilt whenever Minecraft recreates its swapchain.
struct Targets {
    swapchain: vk::SwapchainKHR,
    extent: vk::Extent2D,
    views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    /// One per image: signalled by our submit, waited on by the present.
    semaphores: Vec<vk::Semaphore>,
}

impl Targets {
    unsafe fn destroy(self, device: &ash::Device) {
        for framebuffer in self.framebuffers {
            device.destroy_framebuffer(framebuffer, None);
        }
        for view in self.views {
            device.destroy_image_view(view, None);
        }
        for semaphore in self.semaphores {
            device.destroy_semaphore(semaphore, None);
        }
    }
}

pub struct Overlay {
    device: ash::Device,
    swapchain_fn: ash::khr::swapchain::Device,
    format: vk::Format,
    render_pass: vk::RenderPass,
    command_pool: vk::CommandPool,
    slots: Vec<Slot>,
    next_slot: usize,
    renderer: Option<Renderer>,
    targets: Option<Targets>,
}

fn is_srgb(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::R8G8B8A8_SRGB | vk::Format::B8G8R8A8_SRGB | vk::Format::A8B8G8R8_SRGB_PACK32
    )
}

impl Overlay {
    /// # Safety
    /// `handles` must be live objects of the running game.
    pub unsafe fn new(handles: &Handles) -> anyhow::Result<Overlay> {
        let entry = ash::Entry::from_static_fn(ash::StaticFn {
            get_instance_proc_addr: handles.get_instance_proc_addr,
        });
        let instance = ash::Instance::load(entry.static_fn(), handles.instance);
        let device = ash::Device::load(instance.fp_v1_0(), handles.device);
        let mut overlay = Overlay {
            swapchain_fn: ash::khr::swapchain::Device::new(&instance, &device),
            device,
            format: handles.format,
            render_pass: vk::RenderPass::null(),
            command_pool: vk::CommandPool::null(),
            slots: Vec::new(),
            next_slot: 0,
            renderer: None,
            targets: None,
        };
        match overlay.create_objects(&instance, handles) {
            Ok(()) => Ok(overlay),
            Err(e) => {
                overlay.destroy_objects();
                Err(e)
            }
        }
    }

    unsafe fn create_objects(
        &mut self,
        instance: &ash::Instance,
        handles: &Handles,
    ) -> anyhow::Result<()> {
        let attachments = [vk::AttachmentDescription::default()
            .format(self.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            // Draw over the finished frame, and hand it back ready to present.
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let color_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)];
        // Chains with the semaphore wait at COLOR_ATTACHMENT_OUTPUT, so the
        // layout transition happens only once Minecraft's frame is done.
        let dependencies = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )];
        self.render_pass = self.device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies),
            None,
        )?;

        self.command_pool = self.device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(handles.queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        let command_buffers = self.device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(self.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(SLOTS as u32),
        )?;
        for command_buffer in command_buffers {
            let fence = self.device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?;
            self.slots.push(Slot {
                command_buffer,
                fence,
            });
        }

        self.renderer = Some(
            Renderer::with_default_allocator(
                instance,
                handles.physical_device,
                self.device.clone(),
                self.render_pass,
                Options {
                    in_flight_frames: SLOTS,
                    srgb_framebuffer: is_srgb(self.format),
                    ..Default::default()
                },
            )
            .context("creating the egui Vulkan renderer")?,
        );
        Ok(())
    }

    /// The swapchain the per-image objects were built for.
    pub fn swapchain(&self) -> vk::SwapchainKHR {
        self.targets
            .as_ref()
            .map_or(vk::SwapchainKHR::null(), |targets| targets.swapchain)
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.targets
            .as_ref()
            .map_or(vk::Extent2D::default(), |targets| targets.extent)
    }

    /// Rebuilds the per-image objects for a new swapchain.
    ///
    /// # Safety
    /// Must run on the render thread, inside the present call, which owns
    /// `queue`.
    pub unsafe fn set_swapchain(
        &mut self,
        queue: vk::Queue,
        swapchain: vk::SwapchainKHR,
        extent: vk::Extent2D,
    ) -> anyhow::Result<()> {
        if let Some(old) = self.targets.take() {
            // The old semaphores may still be waited on by a pending present.
            self.device.queue_wait_idle(queue)?;
            old.destroy(&self.device);
        }

        let images = self.swapchain_fn.get_swapchain_images(swapchain)?;
        let mut targets = Targets {
            swapchain,
            extent,
            views: Vec::with_capacity(images.len()),
            framebuffers: Vec::with_capacity(images.len()),
            semaphores: Vec::with_capacity(images.len()),
        };
        if let Err(e) = self.fill_targets(&mut targets, &images) {
            targets.destroy(&self.device);
            return Err(e);
        }
        self.targets = Some(targets);
        Ok(())
    }

    unsafe fn fill_targets(
        &self,
        targets: &mut Targets,
        images: &[vk::Image],
    ) -> anyhow::Result<()> {
        for &image in images {
            let view = self.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?;
            targets.views.push(view);

            let attachments = [view];
            targets.framebuffers.push(
                self.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(self.render_pass)
                        .attachments(&attachments)
                        .width(targets.extent.width)
                        .height(targets.extent.height)
                        .layers(1),
                    None,
                )?,
            );

            targets.semaphores.push(
                self.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
            );
        }
        Ok(())
    }

    /// Draws `frame` onto swapchain image `index`. Returns the semaphore the
    /// present must wait on instead of `waits` — which our submit consumes —
    /// or `None` when there was nothing to draw and `waits` are untouched.
    ///
    /// # Safety
    /// Must run on the render thread, inside the present call, which owns
    /// `queue`.
    pub unsafe fn draw(
        &mut self,
        queue: vk::Queue,
        index: u32,
        waits: &[vk::Semaphore],
        frame: &EguiFrame,
    ) -> anyhow::Result<Option<vk::Semaphore>> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| anyhow!("renderer missing"))?;
        let targets = self
            .targets
            .as_ref()
            .ok_or_else(|| anyhow!("no swapchain targets"))?;

        if !frame.textures_delta.set.is_empty() {
            renderer.set_textures(queue, self.command_pool, &frame.textures_delta.set)?;
        }

        let mut signalled = None;
        if !frame.primitives.is_empty() {
            let index = index as usize;
            let framebuffer = *targets
                .framebuffers
                .get(index)
                .ok_or_else(|| anyhow!("image index {index} out of range"))?;
            let semaphore = targets.semaphores[index];
            let slot = &self.slots[self.next_slot];

            // The slot's command buffer and the renderer's matching vertex
            // buffers are free again once its previous submit has finished.
            self.device
                .wait_for_fences(&[slot.fence], true, FENCE_TIMEOUT_NS)?;
            self.device
                .reset_command_buffer(slot.command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.device.begin_command_buffer(
                slot.command_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            self.device.cmd_begin_render_pass(
                slot.command_buffer,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: targets.extent,
                    }),
                vk::SubpassContents::INLINE,
            );
            renderer.cmd_draw(
                slot.command_buffer,
                targets.extent,
                frame.pixels_per_point,
                &frame.primitives,
            )?;
            self.device.cmd_end_render_pass(slot.command_buffer);
            self.device.end_command_buffer(slot.command_buffer)?;

            let stages = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT; waits.len()];
            let command_buffers = [slot.command_buffer];
            let signals = [semaphore];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signals);
            self.device.reset_fences(&[slot.fence])?;
            self.device.queue_submit(queue, &[submit], slot.fence)?;
            self.next_slot = (self.next_slot + 1) % SLOTS;
            signalled = Some(semaphore);
        }

        if !frame.textures_delta.free.is_empty() {
            // Freed textures may still be sampled by frames in flight.
            let fences: Vec<vk::Fence> = self.slots.iter().map(|slot| slot.fence).collect();
            self.device
                .wait_for_fences(&fences, true, FENCE_TIMEOUT_NS)?;
            renderer.free_textures(&frame.textures_delta.free)?;
        }

        Ok(signalled)
    }

    /// Waits for the GPU and destroys everything the overlay created.
    ///
    /// # Safety
    /// Must run on the render thread, inside the present call, which owns
    /// `queue`.
    pub unsafe fn destroy(mut self, queue: vk::Queue) {
        if let Err(e) = self.device.queue_wait_idle(queue) {
            log::warn!("Vulkan overlay: queue wait failed during teardown: {e}");
        }
        self.destroy_objects();
    }

    /// Destroys every object created so far. Null handles are no-ops in
    /// Vulkan, so this is safe on a partially built overlay.
    unsafe fn destroy_objects(&mut self) {
        self.renderer = None;
        if let Some(targets) = self.targets.take() {
            targets.destroy(&self.device);
        }
        for slot in self.slots.drain(..) {
            self.device.destroy_fence(slot.fence, None);
        }
        // Destroying the pool frees its command buffers.
        self.device.destroy_command_pool(self.command_pool, None);
        self.device.destroy_render_pass(self.render_pass, None);
    }
}
