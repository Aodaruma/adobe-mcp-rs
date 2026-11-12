use super::*;

#[test]
fn process_binding_requires_a_supported_instance_created_during_that_process_lifetime() {
    let process = AeProcess {
        pid: 42,
        started_ms: 1_000_450,
        executable: "AfterFX.exe".into(),
    };
    let mut instance: HostInstance = serde_json::from_value(json!({
        "instanceId":"ae-25.6.6x4-1000500-12", "hostId":"aftereffects",
        "appVersion":"25.6.6x4", "bridgeRuntime":"extendscript-startup",
        "lifecycleMode":"startup-headless", "runtimeId":"runtime-test",
        "bridgeRoot":"bridge", "commandFile":"command", "resultFile":"result",
        "lastHeartbeatAt":"ignored"
    }))
    .unwrap();
    assert!(instance_belongs_to_process(&instance, &process, 1_001_000));
    for (host, version, runtime, lifecycle) in [
        (
            "photoshop",
            Some("25.6.6x4"),
            "extendscript-startup",
            Some("startup-headless"),
        ),
        (
            "aftereffects",
            Some("26.0"),
            "extendscript-startup",
            Some("startup-headless"),
        ),
        (
            "aftereffects",
            None,
            "extendscript-startup",
            Some("startup-headless"),
        ),
        (
            "aftereffects",
            Some("25.6.6x4"),
            "cep",
            Some("startup-headless"),
        ),
        (
            "aftereffects",
            Some("25.6.6x4"),
            "extendscript-startup",
            Some("scriptui-panel"),
        ),
        (
            "aftereffects",
            Some("25.6.6x4"),
            "extendscript-startup",
            None,
        ),
    ] {
        let mut unsupported = instance.clone();
        unsupported.host_id = host.into();
        unsupported.app_version = version.map(Into::into);
        unsupported.bridge_runtime = runtime.into();
        unsupported.lifecycle_mode = lifecycle.map(Into::into);
        assert!(!supports_process_lifetime_tracking(&unsupported));
        assert!(!instance_belongs_to_process(
            &unsupported,
            &process,
            1_001_000
        ));
    }
    for id in [
        "ae-25.6.6x4-1000449-12",
        "ae-25.6.6x4-1001001-12",
        "ae-25.6.6x4-x-12",
        "ae-26.0-1000500-12",
        "ae-25.6.6x4-1000500-x",
    ] {
        instance.instance_id = id.into();
        assert!(
            !instance_belongs_to_process(&instance, &process, 1_001_000),
            "{id}"
        );
    }
    instance.instance_id = "ae-25.6.6x4-1000500-12".into();
    let reused_pid = AeProcess {
        started_ms: 1_000_501,
        ..process.clone()
    };
    assert!(!instance_belongs_to_process(
        &instance,
        &reused_pid,
        1_001_000
    ));
    instance.lifecycle_mode = Some("websocket".into());
    assert!(!instance_belongs_to_process(&instance, &process, 1_001_000));
}
