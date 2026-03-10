use ecotokens::filter::container::filter_container;

#[test]
fn docker_ps_short_passes_through() {
    let input = "CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES\n";
    let out = filter_container("docker ps", input);
    assert!(!out.is_empty(), "should return something");
}

#[test]
fn podman_logs_deduplicates_repeated_lines() {
    let mut input = String::new();
    // Add many repeated lines (need > 50 to trigger deduplication)
    for _ in 0..60 {
        input.push_str("2024-01-01T00:00:00Z INFO heartbeat ok\n");
    }
    input.push_str("2024-01-01T00:01:00Z ERROR something failed\n");
    let out = filter_container("podman logs mycontainer", &input);
    // The repeated lines should be deduplicated
    let heartbeat_count = out.matches("heartbeat ok").count();
    assert!(
        heartbeat_count < 60,
        "repeated lines should be deduplicated, got {} occurrences",
        heartbeat_count
    );
    assert!(out.contains("ERROR"), "error line should be kept");
}

#[test]
fn docker_logs_short_passes_through() {
    let input = "Starting service\nService ready\n";
    let out = filter_container("docker logs myapp", input);
    assert!(
        out.contains("Starting service"),
        "short logs should pass through"
    );
    assert!(out.contains("Service ready"), "all lines should be present");
}

#[test]
fn kubectl_get_short_passes_through() {
    let input =
        "NAME    READY   STATUS    RESTARTS   AGE\nmyapp   1/1     Running   0          5m\n";
    let out = filter_container("kubectl get pods", input);
    assert!(out.contains("Running"), "pod status should be kept");
}

#[test]
fn kubectl_get_many_rows_truncated() {
    let mut input = String::from("NAME    READY   STATUS    RESTARTS   AGE\n");
    for i in 0..200 {
        input.push_str(&format!("pod-{}   1/1     Running   0          5m\n", i));
    }
    let out = filter_container("kubectl get pods", &input);
    assert!(
        out.contains("[ecotokens]"),
        "should have summary marker for many rows"
    );
    assert!(out.contains("omitted"), "should say rows were omitted");
}

#[test]
fn container_unknown_subcommand_uses_generic() {
    let input = "some docker output\n";
    let out = filter_container("docker build .", input);
    assert!(!out.is_empty(), "should return something");
}

#[test]
fn docker_images_short_passes_through() {
    let input = "REPOSITORY   TAG     IMAGE ID      CREATED       SIZE\nnginx        latest  abc123def456  2 weeks ago   142MB\n";
    let out = filter_container("docker images", input);
    assert!(out.contains("nginx"), "image name should be present");
}

#[test]
fn docker_images_compact_format() {
    let mut input = String::from("REPOSITORY         TAG       IMAGE ID      CREATED       SIZE\n");
    for i in 0..10 {
        input.push_str(&format!(
            "myapp/service{}   latest    abc{}def456  3 days ago    250MB\n",
            i, i
        ));
    }
    let out = filter_container("docker images", &input);
    assert!(
        out.contains("myapp/service0:latest"),
        "should use repo:tag format"
    );
    assert!(out.contains("250MB"), "size should be present");
    assert!(!out.contains("abc0def456"), "image ID should be removed");
}

#[test]
fn podman_images_compact_format() {
    let mut input = String::from("REPOSITORY         TAG       IMAGE ID      CREATED       SIZE\n");
    for i in 0..10 {
        input.push_str(&format!(
            "registry/image{}   v1.0    {}abcdef12  1 week ago    180MB\n",
            i, i
        ));
    }
    let out = filter_container("podman images", &input);
    assert!(
        out.contains("registry/image0:v1.0"),
        "should use repo:tag format"
    );
    assert!(out.contains("180MB"), "size should be present");
}
