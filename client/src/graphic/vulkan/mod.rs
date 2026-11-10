//! Vulkan overlay — Minecraft 26.2+ running its Vulkan backend.
//!
//! There is no GL buffer swap to hook under Vulkan. Instead: LWJGL calls
//! `vkQueuePresentKHR` through a function pointer it re-reads on every call
//! from `VKCapabilitiesDevice.vkQueuePresentKHR`. [`install`] swaps that field
//! for [`queue_present`], which runs the per-frame client logic, draws the
//! overlay onto the swapchain image, then presents through the original
//! pointer. [`uninstall`] puts the pointer back.

mod overlay;

use crate::game::prelude::*;
use anyhow::{anyhow, bail};
use ash::vk;
use ash::vk::Handle;
use log::{error, info, warn};
use overlay::{Handles, Overlay};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

struct Hooked {
    /// The `VKCapabilitiesDevice` whose present pointer was swapped.
    capabilities: VKCapabilitiesDevice,
    /// Minecraft's `VulkanGpuSurface`, read for the swapchain size.
    surface: VulkanGpuSurface,
    handles: Handles,
    overlay: Option<Overlay>,
    /// Set once the overlay failed; presents then just pass through.
    disabled: bool,
}

static HOOKED: Mutex<Option<Hooked>> = Mutex::new(None);

/// The original `vkQueuePresentKHR`, readable without taking [`HOOKED`].
static ORIGINAL_PRESENT: AtomicI64 = AtomicI64::new(0);

static INSTALLED: AtomicBool = AtomicBool::new(false);
static TEARDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static TEARDOWN_DONE: AtomicBool = AtomicBool::new(false);

/// Presents currently executing our code — unload must wait for zero.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// `vkGetInstanceProcAddr` from the loader LWJGL itself uses.
fn get_instance_proc_addr() -> anyhow::Result<vk::PFN_vkGetInstanceProcAddr> {
    let pointer = VK::get_function_provider()?.get_function_address("vkGetInstanceProcAddr")?;
    if pointer == 0 {
        bail!("vkGetInstanceProcAddr not found");
    }
    // SAFETY: a non-null `vkGetInstanceProcAddr` from the Vulkan loader.
    Ok(unsafe { std::mem::transmute::<usize, vk::PFN_vkGetInstanceProcAddr>(pointer as usize) })
}

/// Everything [`install`] needs, found through the live game objects.
struct Discovered {
    capabilities: VKCapabilitiesDevice,
    surface: VulkanGpuSurface,
    original_present: i64,
    handles: Handles,
}

/// `None` under OpenGL, or before 26.2.
fn discover() -> anyhow::Result<Option<Discovered>> {
    let frontend = match minecraft().window_surface() {
        Ok(frontend) => frontend,
        Err(mc_bindings::Error::Unavailable { .. }) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(frontend) = frontend.cast::<FrontendGpuSurface>()? else {
        return Ok(None);
    };
    let backend = frontend.backend()?;
    let Some(surface) = backend.cast::<VulkanGpuSurface>()? else {
        return Ok(None);
    };
    info!(
        "Vulkan backend detected ({})",
        backend.runtime_class_name()?
    );

    let queue = surface.present_queue()?;
    let capabilities = queue.get_capabilities()?;
    let original_present = capabilities.vk_queue_present_khr()?;
    if original_present == 0 {
        bail!("vkQueuePresentKHR is not loaded");
    }
    let device = queue.get_device()?;
    let physical_device = device.get_physical_device()?;
    let instance = physical_device.get_instance()?;
    // `presentQueue` is the device's graphics queue; its family is needed to
    // create our command pool.
    let queue_family = surface.device()?.graphics_queue()?.queue_family_index()?;

    let handles = Handles {
        get_instance_proc_addr: get_instance_proc_addr()?,
        instance: vk::Instance::from_raw(instance.address()? as u64),
        physical_device: vk::PhysicalDevice::from_raw(physical_device.address()? as u64),
        device: vk::Device::from_raw(device.address()? as u64),
        queue_family: queue_family as u32,
        format: vk::Format::from_raw(surface.swapchain_image_format()?),
    };
    Ok(Some(Discovered {
        capabilities,
        surface,
        original_present,
        handles,
    }))
}

/// Points LWJGL's present entry back at `pointer`.
fn set_present_pointer(capabilities: &VKCapabilitiesDevice, pointer: i64) -> anyhow::Result<()> {
    Ok(capabilities.set_vk_queue_present_khr(pointer)?)
}

// --- Install / uninstall ---------------------------------------------------

/// Hooks presentation when the game renders through Vulkan. Returns
/// `Ok(false)` — touching nothing — when it renders through OpenGL.
pub fn install() -> anyhow::Result<bool> {
    let Some(discovered) = discover()? else {
        return Ok(false);
    };

    ORIGINAL_PRESENT.store(discovered.original_present, Ordering::SeqCst);
    TEARDOWN_REQUESTED.store(false, Ordering::SeqCst);
    TEARDOWN_DONE.store(false, Ordering::SeqCst);
    let capabilities = discovered.capabilities.clone();
    *HOOKED
        .lock()
        .map_err(|_| anyhow!("Vulkan hook state poisoned"))? = Some(Hooked {
        capabilities: discovered.capabilities,
        surface: discovered.surface,
        handles: discovered.handles,
        overlay: None,
        disabled: false,
    });

    let hook: vk::PFN_vkQueuePresentKHR = queue_present;
    set_present_pointer(&capabilities, hook as usize as i64)?;
    INSTALLED.store(true, Ordering::SeqCst);
    info!(">>> HOOK ACTIVE ON: vkQueuePresentKHR <<<");
    Ok(true)
}

/// Restores the original present pointer and releases the overlay.
///
/// Called from `cleanup_client`, off the render thread. The teardown runs on
/// the render thread at the next present — the only place the queue may be
/// used — and this waits for it. If no frame comes (minimized window), the
/// pointer is restored from here and the GPU objects are leaked rather than
/// destroyed while possibly in use.
pub fn uninstall() {
    if !INSTALLED.swap(false, Ordering::SeqCst) {
        return;
    }
    TEARDOWN_REQUESTED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !TEARDOWN_DONE.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }

    if !TEARDOWN_DONE.load(Ordering::SeqCst) {
        warn!("Vulkan overlay: no frame presented during teardown; leaking its GPU objects");
        if let Ok(mut guard) = HOOKED.lock() {
            if let Some(mut hooked) = guard.take() {
                if let Err(e) = set_present_pointer(
                    &hooked.capabilities,
                    ORIGINAL_PRESENT.load(Ordering::SeqCst),
                ) {
                    error!("Vulkan overlay: could not restore vkQueuePresentKHR: {e}");
                }
                std::mem::forget(hooked.overlay.take());
            }
        }
    }

    // A present that read our pointer just before it was restored may still
    // be on its way into the hook; give it time, then wait for it to leave.
    thread::sleep(Duration::from_millis(20));
    let deadline = Instant::now() + Duration::from_secs(1);
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    info!("Vulkan present hook removed.");
}

// --- The hook --------------------------------------------------------------

unsafe extern "system" fn queue_present(
    queue: vk::Queue,
    present_info: *const vk::PresentInfoKHR<'_>,
) -> vk::Result {
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    let result = present(queue, present_info);
    IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    result
}

unsafe fn present(queue: vk::Queue, present_info: *const vk::PresentInfoKHR<'_>) -> vk::Result {
    let original: vk::PFN_vkQueuePresentKHR =
        std::mem::transmute(ORIGINAL_PRESENT.load(Ordering::SeqCst) as usize);

    if TEARDOWN_REQUESTED.load(Ordering::SeqCst) {
        let result = original(queue, present_info);
        teardown(queue);
        return result;
    }
    if !crate::RUNNING.load(Ordering::SeqCst) || present_info.is_null() {
        return original(queue, present_info);
    }

    crate::graphic::hook::frame_logic();

    let Ok(mut guard) = HOOKED.lock() else {
        return original(queue, present_info);
    };
    let Some(hooked) = guard.as_mut().filter(|hooked| !hooked.disabled) else {
        return original(queue, present_info);
    };

    match draw_overlay(hooked, queue, &*present_info) {
        Ok(Some(semaphore)) => {
            let waits = [semaphore];
            let mut patched = *present_info;
            patched.wait_semaphore_count = 1;
            patched.p_wait_semaphores = waits.as_ptr();
            original(queue, &patched)
        }
        Ok(None) => original(queue, present_info),
        Err(e) => {
            error!("Vulkan overlay disabled: {e:#}");
            hooked.disabled = true;
            original(queue, present_info)
        }
    }
}

/// Lays out and draws one overlay frame. `Ok(Some(_))` means our submit
/// consumed the present's wait semaphores and the present must wait on the
/// returned one instead.
unsafe fn draw_overlay(
    hooked: &mut Hooked,
    queue: vk::Queue,
    info: &vk::PresentInfoKHR<'_>,
) -> anyhow::Result<Option<vk::Semaphore>> {
    if info.swapchain_count != 1 {
        return Ok(None);
    }
    let swapchain = *info.p_swapchains;
    let index = *info.p_image_indices;

    if hooked.overlay.is_none() {
        hooked.overlay = Some(Overlay::new(&hooked.handles)?);
        info!("Vulkan overlay initialized");
    }
    let overlay = hooked.overlay.as_mut().expect("overlay was just created");

    if overlay.swapchain() != swapchain {
        let extent = vk::Extent2D {
            width: hooked.surface.swapchain_width()? as u32,
            height: hooked.surface.swapchain_height()? as u32,
        };
        overlay.set_swapchain(queue, swapchain, extent)?;
    }

    let extent = overlay.extent();
    let frame = crate::graphic::ui_engine::run_frame(extent.width as f32, extent.height as f32);
    let waits = if info.wait_semaphore_count == 0 || info.p_wait_semaphores.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
    };
    overlay.draw(queue, index, waits, &frame)
}

/// Runs on the render thread, right after a pass-through present: puts the
/// original pointer back and destroys the overlay while we hold the queue.
unsafe fn teardown(queue: vk::Queue) {
    if TEARDOWN_DONE.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(mut guard) = HOOKED.lock() {
        if let Some(mut hooked) = guard.take() {
            if let Err(e) = set_present_pointer(
                &hooked.capabilities,
                ORIGINAL_PRESENT.load(Ordering::SeqCst),
            ) {
                error!("Vulkan overlay: could not restore vkQueuePresentKHR: {e}");
            }
            if let Some(overlay) = hooked.overlay.take() {
                overlay.destroy(queue);
            }
        }
    }
    TEARDOWN_DONE.store(true, Ordering::SeqCst);
}
