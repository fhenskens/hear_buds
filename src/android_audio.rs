#[cfg(target_os = "android")]
fn context_local_ref<'a>(env: &mut jni::JNIEnv<'a>) -> Option<jni::objects::JObject<'a>> {
    use jni::objects::{GlobalRef, JObject};
    use jni::sys::jobject;
    use std::sync::{Mutex, OnceLock};

    static APP_CONTEXT: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();

    let cache = APP_CONTEXT.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().ok()?;
    if guard.is_none() {
        let ctx = ndk_context::android_context();
        let context = unsafe { JObject::from_raw(ctx.context() as jobject) };
        let global = match env.new_global_ref(&context) {
            Ok(global) => global,
            Err(_) => {
                std::mem::forget(context);
                return None;
            }
        };
        std::mem::forget(context);
        *guard = Some(global);
    }

    let global = guard.as_ref()?;
    env.new_local_ref(global.as_obj()).ok()
}

#[cfg(target_os = "android")]
pub(crate) fn set_communication_mode(enabled: bool) {
    use jni::objects::JValue;
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(vm) => vm,
        Err(_) => return,
    };
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return,
    };

    let context = match context_local_ref(&mut env) {
        Some(context) => context,
        None => return,
    };

    let context_cls = match env.find_class("android/content/Context") {
        Ok(cls) => cls,
        Err(_) => return,
    };
    let audio_service =
        match env.get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;") {
            Ok(val) => match val.l() {
                Ok(obj) => obj,
                Err(_) => return,
            },
            Err(_) => return,
        };
    let audio_manager = match env.call_method(
        &context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&audio_service)],
    ) {
        Ok(val) => match val.l() {
            Ok(obj) => obj,
            Err(_) => return,
        },
        Err(_) => return,
    };
    let audio_manager = match env.new_local_ref(audio_manager) {
        Ok(obj) => obj,
        Err(_) => return,
    };
    let audio_manager_cls = match env.find_class("android/media/AudioManager") {
        Ok(cls) => cls,
        Err(_) => return,
    };

    let mode = if enabled {
        env.get_static_field(&audio_manager_cls, "MODE_IN_COMMUNICATION", "I")
            .ok()
            .and_then(|value| value.i().ok())
            .unwrap_or(3)
    } else {
        env.get_static_field(&audio_manager_cls, "MODE_NORMAL", "I")
            .ok()
            .and_then(|value| value.i().ok())
            .unwrap_or(0)
    };
    if env
        .call_method(&audio_manager, "setMode", "(I)V", &[JValue::Int(mode)])
        .is_err()
    {
        let _ = env.exception_clear();
    }
}

#[cfg(target_os = "android")]
pub(crate) fn preferred_input_device_id() -> Option<i32> {
    use jni::objects::{JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;

    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    let devices_inputs = env
        .get_static_field(audio_manager_cls, "GET_DEVICES_INPUTS", "I")
        .ok()?
        .i()
        .ok()?;

    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_inputs)],
        )
        .ok()?
        .l()
        .ok()?;

    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;
    if length <= 0 {
        return None;
    }

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|value| value.i().ok());

    let mut best_non_voice: Option<(u8, i32)> = None;
    let mut best_voice: Option<(u8, i32)> = None;

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let device_type = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let device_id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        let is_voice_comm = type_ble_headset == Some(device_type)
            || type_bt_sco == Some(device_type)
            || device_type == 26
            || device_type == 7;

        if is_voice_comm {
            let rank = if device_type == 26 { 0 } else { 1 };
            if best_voice
                .map(|(best_rank, _)| rank < best_rank)
                .unwrap_or(true)
            {
                best_voice = Some((rank, device_id));
            }
        } else {
            let rank = auto_input_type_rank(device_type);
            if best_non_voice
                .map(|(best_rank, _)| rank < best_rank)
                .unwrap_or(true)
            {
                best_non_voice = Some((rank, device_id));
            }
        }
    }

    best_non_voice
        .map(|(_, id)| id)
        .or_else(|| best_voice.map(|(_, id)| id))
}

#[cfg(target_os = "android")]
pub(crate) fn preferred_output_device() -> Option<(i32, bool)> {
    use jni::objects::{JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    let devices_outputs = env
        .get_static_field(&audio_manager_cls, "GET_DEVICES_OUTPUTS", "I")
        .ok()?
        .i()
        .ok()?;

    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_outputs)],
        )
        .ok()?
        .l()
        .ok()?;

    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;
    if length <= 0 {
        return None;
    }

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_ble_speaker = env
        .get_static_field(&device_info_cls, "TYPE_BLE_SPEAKER", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_a2dp = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_A2DP", "I")
        .ok()
        .and_then(|value| value.i().ok());

    let mut a2dp_device: Option<(i32, bool)> = None;
    let mut ble_speaker_device: Option<(i32, bool)> = None;
    let mut ble_headset_device: Option<(i32, bool)> = None;
    let mut sco_device: Option<(i32, bool)> = None;

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let device_type = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let device_id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        if type_bt_a2dp == Some(device_type) {
            a2dp_device = Some((device_id, false));
        } else if type_ble_speaker == Some(device_type) {
            ble_speaker_device = Some((device_id, false));
        } else if type_ble_headset == Some(device_type) {
            ble_headset_device = Some((device_id, true));
        } else if type_bt_sco == Some(device_type) {
            sco_device = Some((device_id, true));
        }
    }

    a2dp_device
        .or(ble_speaker_device)
        .or(ble_headset_device)
        .or(sco_device)
}

#[cfg(target_os = "android")]
pub(crate) fn activate_bluetooth_output(prefer_voice_comm: bool) -> Option<(i32, bool)> {
    use jni::objects::{JObject, JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    if prefer_voice_comm {
        let mode_in_comm = env
            .get_static_field(&audio_manager_cls, "MODE_IN_COMMUNICATION", "I")
            .ok()
            .and_then(|value| value.i().ok())
            .unwrap_or(3);
        if env
            .call_method(
                &audio_manager,
                "setMode",
                "(I)V",
                &[JValue::Int(mode_in_comm)],
            )
            .is_err()
        {
            let _ = env.exception_clear();
        }
    }

    let devices_outputs = env
        .get_static_field(&audio_manager_cls, "GET_DEVICES_OUTPUTS", "I")
        .ok()?
        .i()
        .ok()?;

    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_outputs)],
        )
        .ok()?
        .l()
        .ok()?;

    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;
    if length <= 0 {
        return None;
    }

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_ble_speaker = env
        .get_static_field(&device_info_cls, "TYPE_BLE_SPEAKER", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_a2dp = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_A2DP", "I")
        .ok()
        .and_then(|value| value.i().ok());

    let mut selected_device: Option<JObject> = None;
    let mut selected_type: Option<i32> = None;
    let mut selected_id: Option<i32> = None;

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let device_type = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let device_id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        if prefer_voice_comm {
            if type_ble_headset == Some(device_type) || type_bt_sco == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
                break;
            }
            if selected_device.is_none() && type_bt_a2dp == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
            }
        } else {
            if type_bt_a2dp == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
                break;
            }
            if selected_device.is_none() && type_ble_speaker == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
            } else if selected_device.is_none() && type_ble_headset == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
            } else if selected_device.is_none() && type_bt_sco == Some(device_type) {
                selected_device = Some(device);
                selected_type = Some(device_type);
                selected_id = Some(device_id);
            }
        }
    }

    let Some(device) = selected_device else {
        return None;
    };

    let voice_comm = selected_type == type_bt_sco || selected_type == type_ble_headset;
    if voice_comm
        && env
            .get_method_id(
                &audio_manager_cls,
                "setCommunicationDevice",
                "(Landroid/media/AudioDeviceInfo;)Z",
            )
            .is_ok()
        && env
            .call_method(
                &audio_manager,
                "setCommunicationDevice",
                "(Landroid/media/AudioDeviceInfo;)Z",
                &[JValue::Object(&device)],
            )
            .is_err()
    {
        let _ = env.exception_clear();
    }

    selected_id.map(|id| (id, voice_comm))
}

#[cfg(target_os = "android")]
#[allow(dead_code)]
pub(crate) fn activate_bluetooth_input() -> Option<(i32, bool)> {
    use jni::objects::{JObject, JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;

    let mode_in_comm = env
        .get_static_field(&audio_manager_cls, "MODE_IN_COMMUNICATION", "I")
        .ok()
        .and_then(|value| value.i().ok())
        .unwrap_or(3);
    if env
        .call_method(
            &audio_manager,
            "setMode",
            "(I)V",
            &[JValue::Int(mode_in_comm)],
        )
        .is_err()
    {
        let _ = env.exception_clear();
    }

    let devices_inputs = env
        .get_static_field(&audio_manager_cls, "GET_DEVICES_INPUTS", "I")
        .ok()?
        .i()
        .ok()?;
    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_inputs)],
        )
        .ok()?
        .l()
        .ok()?;
    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;
    if length <= 0 {
        return None;
    }

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|value| value.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|value| value.i().ok());

    let mut selected_device: Option<JObject> = None;
    let mut selected_type: Option<i32> = None;
    let mut selected_id: Option<i32> = None;

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let device_type = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let device_id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        if type_ble_headset == Some(device_type) {
            selected_device = Some(device);
            selected_type = Some(device_type);
            selected_id = Some(device_id);
            break;
        }
        if selected_device.is_none() && type_bt_sco == Some(device_type) {
            selected_device = Some(device);
            selected_type = Some(device_type);
            selected_id = Some(device_id);
        }
    }

    let Some(device) = selected_device else {
        return None;
    };

    let voice_comm = selected_type == type_bt_sco || selected_type == type_ble_headset;
    if voice_comm
        && env
            .get_method_id(
                &audio_manager_cls,
                "setCommunicationDevice",
                "(Landroid/media/AudioDeviceInfo;)Z",
            )
            .is_ok()
        && env
            .call_method(
                &audio_manager,
                "setCommunicationDevice",
                "(Landroid/media/AudioDeviceInfo;)Z",
                &[JValue::Object(&device)],
            )
            .is_err()
    {
        let _ = env.exception_clear();
    }

    if selected_type == type_bt_sco {
        if env
            .call_method(&audio_manager, "startBluetoothSco", "()V", &[])
            .is_err()
        {
            let _ = env.exception_clear();
        }
        if env
            .call_method(
                &audio_manager,
                "setBluetoothScoOn",
                "(Z)V",
                &[JValue::Bool(1)],
            )
            .is_err()
        {
            let _ = env.exception_clear();
        }
    }

    selected_id.map(|id| (id, voice_comm))
}

#[cfg(target_os = "android")]
pub(crate) fn deactivate_bluetooth_input() {
    use jni::objects::JValue;
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(vm) => vm,
        Err(_) => return,
    };
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return,
    };

    let context = match context_local_ref(&mut env) {
        Some(context) => context,
        None => return,
    };

    let context_cls = match env.find_class("android/content/Context") {
        Ok(cls) => cls,
        Err(_) => return,
    };
    let audio_service =
        match env.get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;") {
            Ok(val) => match val.l() {
                Ok(obj) => obj,
                Err(_) => return,
            },
            Err(_) => return,
        };
    let audio_manager = match env.call_method(
        &context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&audio_service)],
    ) {
        Ok(val) => match val.l() {
            Ok(obj) => obj,
            Err(_) => return,
        },
        Err(_) => return,
    };
    let audio_manager = match env.new_local_ref(audio_manager) {
        Ok(obj) => obj,
        Err(_) => return,
    };
    let audio_manager_cls = match env.find_class("android/media/AudioManager") {
        Ok(cls) => cls,
        Err(_) => return,
    };

    if env
        .call_method(&audio_manager, "stopBluetoothSco", "()V", &[])
        .is_err()
    {
        let _ = env.exception_clear();
    }
    if env
        .call_method(
            &audio_manager,
            "setBluetoothScoOn",
            "(Z)V",
            &[JValue::Bool(0)],
        )
        .is_err()
    {
        let _ = env.exception_clear();
    }

    if env
        .get_method_id(&audio_manager_cls, "clearCommunicationDevice", "()V")
        .is_ok()
    {
        if env
            .call_method(&audio_manager, "clearCommunicationDevice", "()V", &[])
            .is_err()
        {
            let _ = env.exception_clear();
        }
    }

    let mode_normal = env
        .get_static_field(&audio_manager_cls, "MODE_NORMAL", "I")
        .ok()
        .and_then(|value| value.i().ok())
        .unwrap_or(0);
    if env
        .call_method(
            &audio_manager,
            "setMode",
            "(I)V",
            &[JValue::Int(mode_normal)],
        )
        .is_err()
    {
        let _ = env.exception_clear();
    }
}

#[cfg(target_os = "android")]
pub(crate) fn describe_input_device(device_id: i32) -> Option<String> {
    describe_device(device_id, false)
}

#[cfg(target_os = "android")]
pub(crate) fn describe_output_device(device_id: i32) -> Option<String> {
    describe_device(device_id, true)
}

#[cfg(target_os = "android")]
pub(crate) fn list_input_devices() -> Vec<(i32, String)> {
    list_devices(false)
}

#[cfg(target_os = "android")]
pub(crate) fn list_output_devices() -> Vec<(i32, String)> {
    list_devices(true)
}

#[cfg(target_os = "android")]
pub(crate) fn activate_input_device(device_id: i32) -> Option<(i32, bool)> {
    use jni::objects::{JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    let devices_inputs = env
        .get_static_field(&audio_manager_cls, "GET_DEVICES_INPUTS", "I")
        .ok()?
        .i()
        .ok()?;
    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_inputs)],
        )
        .ok()?
        .l()
        .ok()?;
    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|v| v.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|v| v.i().ok());

    let mode_in_comm = env
        .get_static_field(&audio_manager_cls, "MODE_IN_COMMUNICATION", "I")
        .ok()
        .and_then(|value| value.i().ok())
        .unwrap_or(3);
    if env
        .call_method(
            &audio_manager,
            "setMode",
            "(I)V",
            &[JValue::Int(mode_in_comm)],
        )
        .is_err()
    {
        let _ = env.exception_clear();
    }

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        if id != device_id {
            continue;
        }
        let ty = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        let voice_comm = type_bt_sco == Some(ty) || type_ble_headset == Some(ty);
        if voice_comm
            && env
                .get_method_id(
                    &audio_manager_cls,
                    "setCommunicationDevice",
                    "(Landroid/media/AudioDeviceInfo;)Z",
                )
                .is_ok()
            && env
                .call_method(
                    &audio_manager,
                    "setCommunicationDevice",
                    "(Landroid/media/AudioDeviceInfo;)Z",
                    &[JValue::Object(&device)],
                )
                .is_err()
        {
            let _ = env.exception_clear();
        }

        if type_bt_sco == Some(ty) {
            if env
                .call_method(&audio_manager, "startBluetoothSco", "()V", &[])
                .is_err()
            {
                let _ = env.exception_clear();
            }
            if env
                .call_method(
                    &audio_manager,
                    "setBluetoothScoOn",
                    "(Z)V",
                    &[JValue::Bool(1)],
                )
                .is_err()
            {
                let _ = env.exception_clear();
            }
        }

        return Some((id, voice_comm));
    }

    None
}

#[cfg(target_os = "android")]
pub(crate) fn activate_output_device(
    device_id: i32,
    prefer_voice_comm: bool,
) -> Option<(i32, bool)> {
    use jni::objects::{JObjectArray, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    if prefer_voice_comm {
        let mode_in_comm = env
            .get_static_field(&audio_manager_cls, "MODE_IN_COMMUNICATION", "I")
            .ok()
            .and_then(|value| value.i().ok())
            .unwrap_or(3);
        if env
            .call_method(
                &audio_manager,
                "setMode",
                "(I)V",
                &[JValue::Int(mode_in_comm)],
            )
            .is_err()
        {
            let _ = env.exception_clear();
        }
    }

    let devices_outputs = env
        .get_static_field(&audio_manager_cls, "GET_DEVICES_OUTPUTS", "I")
        .ok()?
        .i()
        .ok()?;
    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(devices_outputs)],
        )
        .ok()?
        .l()
        .ok()?;
    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;

    let device_info_cls = env.find_class("android/media/AudioDeviceInfo").ok()?;
    let type_ble_headset = env
        .get_static_field(&device_info_cls, "TYPE_BLE_HEADSET", "I")
        .ok()
        .and_then(|v| v.i().ok());
    let type_bt_sco = env
        .get_static_field(&device_info_cls, "TYPE_BLUETOOTH_SCO", "I")
        .ok()
        .and_then(|v| v.i().ok());

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        if id != device_id {
            continue;
        }
        let ty = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        let voice_comm = type_bt_sco == Some(ty) || type_ble_headset == Some(ty);
        if voice_comm
            && env
                .get_method_id(
                    &audio_manager_cls,
                    "setCommunicationDevice",
                    "(Landroid/media/AudioDeviceInfo;)Z",
                )
                .is_ok()
            && env
                .call_method(
                    &audio_manager,
                    "setCommunicationDevice",
                    "(Landroid/media/AudioDeviceInfo;)Z",
                    &[JValue::Object(&device)],
                )
                .is_err()
        {
            let _ = env.exception_clear();
        }

        return Some((id, voice_comm));
    }

    None
}

#[cfg(target_os = "android")]
fn describe_device(device_id: i32, output: bool) -> Option<String> {
    use jni::objects::{JObjectArray, JString, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context = context_local_ref(&mut env)?;

    let context_cls = env.find_class("android/content/Context").ok()?;
    let audio_service = env
        .get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&audio_service)],
        )
        .ok()?
        .l()
        .ok()?;
    let audio_manager = env.new_local_ref(audio_manager).ok()?;
    let audio_manager_cls = env.find_class("android/media/AudioManager").ok()?;
    let device_flag = if output {
        env.get_static_field(&audio_manager_cls, "GET_DEVICES_OUTPUTS", "I")
            .ok()?
            .i()
            .ok()?
    } else {
        env.get_static_field(&audio_manager_cls, "GET_DEVICES_INPUTS", "I")
            .ok()?
            .i()
            .ok()?
    };

    let devices = env
        .call_method(
            &audio_manager,
            "getDevices",
            "(I)[Landroid/media/AudioDeviceInfo;",
            &[JValue::Int(device_flag)],
        )
        .ok()?
        .l()
        .ok()?;
    let devices = JObjectArray::from(devices);
    let length = env.get_array_length(&devices).ok()? as i32;

    for index in 0..length {
        let device = env.get_object_array_element(&devices, index).ok()?;
        let device = env.new_local_ref(device).ok()?;
        let id = env
            .call_method(&device, "getId", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        if id != device_id {
            continue;
        }
        let ty = env
            .call_method(&device, "getType", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let name = env
            .call_method(&device, "getProductName", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok())
            .and_then(|char_seq| {
                env.call_method(&char_seq, "toString", "()Ljava/lang/String;", &[])
                    .ok()
                    .and_then(|v| v.l().ok())
            })
            .and_then(|s| {
                let js = JString::from(s);
                env.get_string(&js)
                    .ok()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Unknown".to_string());

        return Some(format!("{name} (id={id}, type={ty})"));
    }

    None
}

#[cfg(target_os = "android")]
fn list_devices(output: bool) -> Vec<(i32, String)> {
    use jni::objects::{JObjectArray, JString, JValue};
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(vm) => vm,
        Err(_) => return Vec::new(),
    };
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return Vec::new(),
    };

    let context = match context_local_ref(&mut env) {
        Some(context) => context,
        None => return Vec::new(),
    };

    let context_cls = match env.find_class("android/content/Context") {
        Ok(cls) => cls,
        Err(_) => return Vec::new(),
    };
    let audio_service =
        match env.get_static_field(context_cls, "AUDIO_SERVICE", "Ljava/lang/String;") {
            Ok(value) => match value.l() {
                Ok(obj) => obj,
                Err(_) => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        };
    let audio_manager = match env.call_method(
        &context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&audio_service)],
    ) {
        Ok(value) => match value.l() {
            Ok(obj) => obj,
            Err(_) => return Vec::new(),
        },
        Err(_) => return Vec::new(),
    };
    let audio_manager = match env.new_local_ref(audio_manager) {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };
    let audio_manager_cls = match env.find_class("android/media/AudioManager") {
        Ok(cls) => cls,
        Err(_) => return Vec::new(),
    };
    let device_flag = if output {
        match env.get_static_field(&audio_manager_cls, "GET_DEVICES_OUTPUTS", "I") {
            Ok(value) => match value.i() {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        }
    } else {
        match env.get_static_field(&audio_manager_cls, "GET_DEVICES_INPUTS", "I") {
            Ok(value) => match value.i() {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        }
    };

    let devices = match env.call_method(
        &audio_manager,
        "getDevices",
        "(I)[Landroid/media/AudioDeviceInfo;",
        &[JValue::Int(device_flag)],
    ) {
        Ok(value) => match value.l() {
            Ok(obj) => obj,
            Err(_) => return Vec::new(),
        },
        Err(_) => return Vec::new(),
    };
    let devices = JObjectArray::from(devices);
    let length = match env.get_array_length(&devices) {
        Ok(v) => v as i32,
        Err(_) => return Vec::new(),
    };

    let mut results: Vec<(i32, i32, String)> = Vec::with_capacity(length as usize);
    for index in 0..length {
        let device = match env.get_object_array_element(&devices, index) {
            Ok(obj) => obj,
            Err(_) => continue,
        };
        let device = match env.new_local_ref(device) {
            Ok(obj) => obj,
            Err(_) => continue,
        };
        let id = match env.call_method(&device, "getId", "()I", &[]) {
            Ok(v) => match v.i() {
                Ok(id) => id,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let ty = match env.call_method(&device, "getType", "()I", &[]) {
            Ok(v) => match v.i() {
                Ok(ty) => ty,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let name = env
            .call_method(&device, "getProductName", "()Ljava/lang/CharSequence;", &[])
            .ok()
            .and_then(|v| v.l().ok())
            .and_then(|char_seq| {
                env.call_method(&char_seq, "toString", "()Ljava/lang/String;", &[])
                    .ok()
                    .and_then(|v| v.l().ok())
            })
            .and_then(|s| {
                let js = JString::from(s);
                env.get_string(&js)
                    .ok()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Unknown".to_string());
        let type_label = device_type_label(ty);
        let friendly = if name.trim().is_empty() || name == "Unknown" {
            type_label.to_string()
        } else {
            name
        };
        results.push((id, ty, format!("{friendly} ({type_label})")));
    }
    results.sort_by(|a, b| {
        let left = device_sort_key(output, a.1, &a.2);
        let right = device_sort_key(output, b.1, &b.2);
        left.cmp(&right).then_with(|| a.0.cmp(&b.0))
    });
    results
        .into_iter()
        .map(|(id, _ty, label)| (id, label))
        .collect()
}

#[cfg(target_os = "android")]
fn device_type_label(ty: i32) -> &'static str {
    match ty {
        1 => "Earpiece",
        2 => "Speaker",
        3 => "Wired headset",
        4 => "Wired headphones",
        7 => "Bluetooth SCO",
        8 => "Bluetooth A2DP",
        15 => "Built-in mic",
        16 => "FM tuner",
        18 => "USB audio",
        22 => "USB headset",
        23 => "Hearing aid",
        26 => "BLE headset",
        27 => "BLE speaker",
        28 => "BLE broadcast",
        _ => "Audio device",
    }
}

#[cfg(target_os = "android")]
fn device_sort_key(output: bool, ty: i32, label: &str) -> (u8, u8, u8, String) {
    let is_sco = ty == 7;
    let peripheral_rank = if is_peripheral_device(output, ty) {
        0
    } else {
        1
    };
    (
        if is_sco { 1 } else { 0 },
        peripheral_rank,
        device_type_rank(output, ty),
        label.to_ascii_lowercase(),
    )
}

#[cfg(target_os = "android")]
fn is_peripheral_device(output: bool, ty: i32) -> bool {
    if output {
        matches!(ty, 3 | 4 | 7 | 8 | 18 | 22 | 23 | 26 | 27 | 28)
    } else {
        matches!(ty, 3 | 7 | 18 | 22 | 23 | 26)
    }
}

#[cfg(target_os = "android")]
fn device_type_rank(output: bool, ty: i32) -> u8 {
    if output {
        match ty {
            8 => 0,   // Bluetooth A2DP
            27 => 1,  // BLE speaker
            23 => 2,  // Hearing aid
            4 => 3,   // Wired headphones
            3 => 4,   // Wired headset
            22 => 5,  // USB headset
            18 => 6,  // USB audio
            2 => 7,   // Speaker
            1 => 8,   // Earpiece
            26 => 9,  // BLE headset
            28 => 10, // BLE broadcast
            7 => 11,  // Bluetooth SCO (already grouped last)
            _ => 12,
        }
    } else {
        match ty {
            26 => 0, // BLE headset mic
            7 => 1,  // Bluetooth SCO mic
            22 => 2, // USB headset
            18 => 3, // USB audio
            3 => 4,  // Wired headset mic
            15 => 5, // Built-in mic
            _ => 6,
        }
    }
}

#[cfg(target_os = "android")]
fn auto_input_type_rank(ty: i32) -> u8 {
    match ty {
        3 => 0,   // Wired headset mic
        22 => 1,  // USB headset
        18 => 2,  // USB audio
        15 => 3,  // Built-in mic
        23 => 4,  // Hearing aid input
        26 => 10, // BLE headset mic (voice comm class)
        7 => 11,  // Bluetooth SCO mic
        _ => 5,
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) fn set_communication_mode(_enabled: bool) {}

#[cfg(not(target_os = "android"))]
pub(crate) fn preferred_input_device_id() -> Option<i32> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn preferred_output_device() -> Option<(i32, bool)> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn activate_bluetooth_output(_prefer_voice_comm: bool) -> Option<(i32, bool)> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn activate_bluetooth_input() -> Option<(i32, bool)> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn deactivate_bluetooth_input() {}

#[cfg(not(target_os = "android"))]
pub(crate) fn describe_input_device(_device_id: i32) -> Option<String> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn describe_output_device(_device_id: i32) -> Option<String> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn list_input_devices() -> Vec<(i32, String)> {
    Vec::new()
}

#[cfg(not(target_os = "android"))]
pub(crate) fn list_output_devices() -> Vec<(i32, String)> {
    Vec::new()
}

#[cfg(not(target_os = "android"))]
pub(crate) fn activate_input_device(_device_id: i32) -> Option<(i32, bool)> {
    None
}

#[cfg(not(target_os = "android"))]
pub(crate) fn activate_output_device(
    _device_id: i32,
    _prefer_voice_comm: bool,
) -> Option<(i32, bool)> {
    None
}
