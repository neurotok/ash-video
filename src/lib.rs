use ash::extensions::{
    ext::DebugUtils,
    khr::{Surface, Swapchain},
};

use ash::{vk, Entry};
pub use ash::{Device, Instance};

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use anyhow::Result;

use winit;

use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_char;

const VALIDATION_ENABLED: bool = cfg!(debug_assertions);

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{:?}:\n{:?} [{} ({})] : {}\n",
        message_severity, message_type, message_id_name, message_id_number, message,
    );

    vk::FALSE
}

pub struct App {
    entry: Entry,
    app_data: AppData,
    instance: Instance,
    device: Device,
}

impl App {
    pub unsafe fn create(window: &winit::window::Window) -> Result<Self> {
        let entry = Entry::linked();

        let mut app_data = AppData::default();

        let instance = create_instance(window, &entry, &mut app_data)?;

        app_data.surface = ash_window::create_surface(
            &entry,
            &instance,
            window.raw_display_handle(),
            window.raw_window_handle(),
            None,
        )?;

        let video_extensions = CStr::from_bytes_with_nul_unchecked(b"VK_KHR_video_queue\0");

        let device = create_device(&instance, &entry, &mut app_data)?;

        Ok(Self {
            entry,
            app_data,
            instance,
            device,
        })
    }
    pub unsafe fn render(&mut self, window: &winit::window::Window) -> Result<()> {
        Ok(())
    }
    pub unsafe fn destroy(&mut self) {
        self.instance.destroy_instance(None);
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppData {
    pub debug_call_back: vk::DebugUtilsMessengerEXT,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub graphics_queue: vk::Queue,
}

pub unsafe fn create_instance(
    window: &winit::window::Window,
    entry: &Entry,
    app_data: &mut AppData,
) -> Result<Instance> {
    let app_name = CStr::from_bytes_with_nul_unchecked(b"VulkanTriangle\0");

    let layer_names = [CStr::from_bytes_with_nul_unchecked(
        b"VK_LAYER_KHRONOS_validation\0",
    )];
    let layers_names_raw: Vec<*const c_char> = layer_names
        .iter()
        .map(|raw_name| raw_name.as_ptr())
        .collect();

    let mut extension_names =
        ash_window::enumerate_required_extensions(window.raw_display_handle())
            .unwrap()
            .to_vec();

    extension_names.push(DebugUtils::name().as_ptr());

    let appinfo = vk::ApplicationInfo::builder()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(vk::make_api_version(0, 1, 0, 0));

    let create_flags = vk::InstanceCreateFlags::default();

    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&appinfo)
        .enabled_layer_names(&layers_names_raw)
        .enabled_extension_names(&extension_names)
        .flags(create_flags);

    let instance: Instance = entry
        .create_instance(&create_info, None)
        .expect("Instance creation error");

    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));

    let debug_utils_loader = DebugUtils::new(&entry, &instance);

    if VALIDATION_ENABLED {
        app_data.debug_call_back =
            debug_utils_loader.create_debug_utils_messenger(&debug_info, None)?
    }

    Ok(instance)
}

pub unsafe fn create_device(
    instance: &Instance,
    entry: &Entry,
    app_data: &mut AppData,
) -> Result<Device> {
    let pdevices = instance
        .enumerate_physical_devices()
        .expect("Physical device error");
    let surface_loader = Surface::new(&entry, &instance);
    let (pdevice, queue_family_index) = pdevices
        .iter()
        .find_map(|pdevice| {
            instance
                .get_physical_device_queue_family_properties(*pdevice)
                .iter()
                .enumerate()
                .find_map(|(index, info)| {
                    let supports_graphic_and_surface =
                        info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                            && surface_loader
                                .get_physical_device_surface_support(
                                    *pdevice,
                                    index as u32,
                                    app_data.surface,
                                )
                                .unwrap();
                    if supports_graphic_and_surface {
                        Some((*pdevice, index))
                    } else {
                        None
                    }
                })
        })
        .expect("Couldn't find suitable device.");
    let queue_family_index = queue_family_index as u32;

    let device_extension_names_raw = [
        Swapchain::name().as_ptr(),
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        KhrPortabilitySubsetFn::name().as_ptr(),
    ];
    let features = vk::PhysicalDeviceFeatures {
        shader_clip_distance: 1,
        ..Default::default()
    };
    let priorities = [1.0];

    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_family_index)
        .queue_priorities(&priorities);

    let device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_extension_names_raw)
        .enabled_features(&features);

    let device: Device = instance
        .create_device(pdevice, &device_create_info, None)
        .unwrap();

    Ok(device)
}
