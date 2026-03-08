#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
}

#[cfg(target_os = "android")]
fn clear_jni_exception(env: &mut jni::JNIEnv<'_>) {
    let _ = env.exception_clear();
}

#[cfg(target_os = "android")]
fn with_android_context<T>(f: impl FnOnce(&mut jni::JNIEnv<'_>, jni::objects::JObject<'_>) -> T) -> Option<T> {
    use jni::objects::JObject;
    use jni::sys::jobject;
    use jni::JavaVM;

    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let context_raw = unsafe { JObject::from_raw(ctx.context() as jobject) };
    let context = match env.new_local_ref(&context_raw) {
        Ok(local) => {
            std::mem::forget(context_raw);
            local
        }
        Err(_) => {
            clear_jni_exception(&mut env);
            return None;
        }
    };

    Some(f(&mut env, context))
}

#[cfg(target_os = "android")]
fn load_bridge_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    context: &jni::objects::JObject<'a>,
) -> Option<jni::objects::JClass<'a>> {
    use jni::objects::{JClass, JObject, JString, JValue};

    let class_loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .ok()?
        .l()
        .ok()?;

    let class_name: JString = env.new_string("dev.dioxus.main.PermissionBridge").ok()?;
    let class_obj = env
        .call_method(
            &class_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&JObject::from(class_name))],
        )
        .ok()?
        .l()
        .ok()?;

    Some(JClass::from(class_obj))
}

pub fn check_microphone_permission() -> PermissionState {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;

        let granted = with_android_context(|env, context| {
            let Some(bridge_class) = load_bridge_class(env, &context) else {
                clear_jni_exception(env);
                return false;
            };

            match env.call_static_method(
                bridge_class,
                "hasRecordAudioPermission",
                "(Landroid/content/Context;)Z",
                &[JValue::Object(&context)],
            ) {
                Ok(value) => value.z().unwrap_or(false),
                Err(_) => {
                    clear_jni_exception(env);
                    false
                }
            }
        })
        .unwrap_or(false);

        if granted {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        PermissionState::Granted
    }
}

pub fn request_microphone_permission() -> PermissionState {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;

        const REQUEST_CODE: i32 = 1101;

        if check_microphone_permission() == PermissionState::Granted {
            return PermissionState::Granted;
        }

        let _ = with_android_context(|env, context| {
            let Some(bridge_class) = load_bridge_class(env, &context) else {
                clear_jni_exception(env);
                return;
            };

            if env
                .call_static_method(
                    bridge_class,
                    "requestRecordAudioPermission",
                    "(Landroid/content/Context;I)Z",
                    &[JValue::Object(&context), JValue::Int(REQUEST_CODE)],
                )
                .is_err()
            {
                clear_jni_exception(env);
            }
        });

        PermissionState::Denied
    }

    #[cfg(not(target_os = "android"))]
    {
        PermissionState::Granted
    }
}
