#[cfg(target_os = "android")]
fn clear_jni_exception(env: &mut jni::JNIEnv<'_>) {
    let _ = env.exception_clear();
}

#[cfg(target_os = "android")]
pub(crate) fn set_dsp_foreground_service_enabled(enabled: bool) -> bool {
    use jni::objects::{JClass, JObject, JString, JValue};
    use jni::sys::jobject;
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(vm) => vm,
        Err(_) => return false,
    };
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => return false,
    };

    let context_raw = unsafe { JObject::from_raw(ctx.context() as jobject) };
    let context = match env.new_local_ref(&context_raw) {
        Ok(local) => {
            std::mem::forget(context_raw);
            local
        }
        Err(_) => {
            clear_jni_exception(&mut env);
            return false;
        }
    };

    let class_loader =
        match env.call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[]) {
            Ok(value) => match value.l() {
                Ok(loader) => loader,
                Err(_) => {
                    clear_jni_exception(&mut env);
                    return false;
                }
            },
            Err(_) => {
                clear_jni_exception(&mut env);
                return false;
            }
        };
    let service_name: JString = match env.new_string("dev.dioxus.main.HearBudsForegroundService") {
        Ok(value) => value,
        Err(_) => {
            clear_jni_exception(&mut env);
            return false;
        }
    };
    let service_class_obj = match env.call_method(
        &class_loader,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&JObject::from(service_name))],
    ) {
        Ok(value) => match value.l() {
            Ok(class_obj) => class_obj,
            Err(_) => {
                clear_jni_exception(&mut env);
                return false;
            }
        },
        Err(_) => {
            clear_jni_exception(&mut env);
            return false;
        }
    };
    let service_cls = JClass::from(service_class_obj);

    let intent = match env.new_object(
        "android/content/Intent",
        "(Landroid/content/Context;Ljava/lang/Class;)V",
        &[
            JValue::Object(&context),
            JValue::Object(&JObject::from(service_cls)),
        ],
    ) {
        Ok(intent) => intent,
        Err(_) => {
            clear_jni_exception(&mut env);
            return false;
        }
    };

    if enabled {
        let sdk_int = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
            .ok()
            .and_then(|v| v.i().ok())
            .unwrap_or(21);
        let method_name = if sdk_int >= 26 {
            "startForegroundService"
        } else {
            "startService"
        };
        if env
            .call_method(
                &context,
                method_name,
                "(Landroid/content/Intent;)Landroid/content/ComponentName;",
                &[JValue::Object(&intent)],
            )
            .is_err()
        {
            clear_jni_exception(&mut env);
            return false;
        }
    } else if env
        .call_method(
            &context,
            "stopService",
            "(Landroid/content/Intent;)Z",
            &[JValue::Object(&intent)],
        )
        .is_err()
    {
        clear_jni_exception(&mut env);
        return false;
    }

    true
}

#[cfg(not(target_os = "android"))]
pub(crate) fn set_dsp_foreground_service_enabled(_enabled: bool) -> bool {
    false
}

#[cfg(target_os = "android")]
pub(crate) fn request_ignore_battery_optimizations_if_needed() {
    use jni::objects::{JObject, JString, JValue};
    use jni::sys::jobject;
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

    let context_raw = unsafe { JObject::from_raw(ctx.context() as jobject) };
    let context = match env.new_local_ref(&context_raw) {
        Ok(local) => {
            std::mem::forget(context_raw);
            local
        }
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };

    let package_name: JString =
        match env.call_method(&context, "getPackageName", "()Ljava/lang/String;", &[]) {
            Ok(value) => match value.l() {
                Ok(obj) => JString::from(obj),
                Err(_) => {
                    clear_jni_exception(&mut env);
                    return;
                }
            },
            Err(_) => {
                clear_jni_exception(&mut env);
                return;
            }
        };
    let package_name_text = match env.get_string(&package_name) {
        Ok(value) => value.to_string_lossy().into_owned(),
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };
    let package_name_obj = JObject::from(package_name);

    let power_service_name = match env.new_string("power") {
        Ok(value) => value,
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };
    let power_service = match env.call_method(
        &context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&JObject::from(power_service_name))],
    ) {
        Ok(value) => match value.l() {
            Ok(obj) => obj,
            Err(_) => {
                clear_jni_exception(&mut env);
                return;
            }
        },
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };

    let is_ignored = match env.call_method(
        &power_service,
        "isIgnoringBatteryOptimizations",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&package_name_obj)],
    ) {
        Ok(value) => value.z().unwrap_or(false),
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };

    if is_ignored {
        return;
    }

    let action = match env.new_string("android.settings.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS") {
        Ok(value) => value,
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };
    let intent = match env.new_object(
        "android/content/Intent",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&JObject::from(action))],
    ) {
        Ok(intent) => intent,
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };

    let uri_text = match env.new_string(format!("package:{package_name_text}")) {
        Ok(value) => value,
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };
    let uri_obj = match env.call_static_method(
        "android/net/Uri",
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[JValue::Object(&JObject::from(uri_text))],
    ) {
        Ok(value) => match value.l() {
            Ok(obj) => obj,
            Err(_) => {
                clear_jni_exception(&mut env);
                return;
            }
        },
        Err(_) => {
            clear_jni_exception(&mut env);
            return;
        }
    };

    if env
        .call_method(
            &intent,
            "setData",
            "(Landroid/net/Uri;)Landroid/content/Intent;",
            &[JValue::Object(&uri_obj)],
        )
        .is_err()
    {
        clear_jni_exception(&mut env);
        return;
    }

    let flag_new_task = 0x1000_0000i32;
    if env
        .call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(flag_new_task)],
        )
        .is_err()
    {
        clear_jni_exception(&mut env);
        return;
    }

    if env
        .call_method(
            &context,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )
        .is_err()
    {
        clear_jni_exception(&mut env);
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) fn request_ignore_battery_optimizations_if_needed() {}
