#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
}

pub fn check_microphone_permission() -> PermissionState {
    #[cfg(target_os = "android")]
    {
        use jni::objects::{JObject, JValue};
        use jni::sys::jobject;
        use jni::JavaVM;

        const PERMISSION_GRANTED: i32 = 0;

        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
        let Ok(vm) = vm else {
            return PermissionState::Denied;
        };

        let env = vm.attach_current_thread();
        let Ok(mut env) = env else {
            return PermissionState::Denied;
        };

        let context = unsafe { JObject::from_raw(ctx.context() as jobject) };
        let context = match env.new_local_ref(&context) {
            Ok(local) => {
                std::mem::forget(context);
                local
            }
            Err(_) => return PermissionState::Denied,
        };
        let permission = env.new_string("android.permission.RECORD_AUDIO");
        let Ok(permission) = permission else {
            return PermissionState::Denied;
        };

        let permission_obj = JObject::from(permission);
        let check = env.call_method(
            &context,
            "checkSelfPermission",
            "(Ljava/lang/String;)I",
            &[JValue::Object(&permission_obj)],
        );

        if let Ok(result) = check {
            if let Ok(status) = result.i() {
                if status == PERMISSION_GRANTED {
                    return PermissionState::Granted;
                }
            }
        }

        PermissionState::Denied
    }

    #[cfg(not(target_os = "android"))]
    {
        PermissionState::Granted
    }
}

pub fn request_microphone_permission() -> PermissionState {
    #[cfg(target_os = "android")]
    {
        use jni::objects::{JObject, JValue};
        use jni::sys::jobject;
        use jni::JavaVM;

        const REQUEST_CODE: i32 = 1101;

        if check_microphone_permission() == PermissionState::Granted {
            return PermissionState::Granted;
        }

        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
        let Ok(vm) = vm else {
            return PermissionState::Denied;
        };

        let env = vm.attach_current_thread();
        let Ok(mut env) = env else {
            return PermissionState::Denied;
        };

        let context = unsafe { JObject::from_raw(ctx.context() as jobject) };
        let context = match env.new_local_ref(&context) {
            Ok(local) => {
                std::mem::forget(context);
                local
            }
            Err(_) => return PermissionState::Denied,
        };
        let permission = env.new_string("android.permission.RECORD_AUDIO");
        let Ok(permission) = permission else {
            return PermissionState::Denied;
        };

        let permission_obj = JObject::from(permission);
        let string_class = env.find_class("java/lang/String");
        let Ok(string_class) = string_class else {
            return PermissionState::Denied;
        };

        let permissions = env.new_object_array(1, string_class, permission_obj);
        let Ok(permissions) = permissions else {
            return PermissionState::Denied;
        };

        let _ = env.call_method(
            &context,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[JValue::Object(&permissions), JValue::Int(REQUEST_CODE)],
        );

        PermissionState::Denied
    }

    #[cfg(not(target_os = "android"))]
    {
        PermissionState::Granted
    }
}
