use crate::helpers::spawn_app;

// `tokio::test` is the testing equivalent of `tokio::main`.
// It also spares you from having to specify the `#[test]` attribute.
//
// You can inspect what code gets generated using
// `cargo expand --test health_check` (<- name of the test file)
#[tokio::test]
async fn health_check_works() {
    //Arrange
    // spawn_app is the only piece that will, reasonably, depend on our application code.
    let app = spawn_app().await;
    // We need to bring in `reqwest` to perform HTTP requests against our application
    let client = reqwest::Client::new();

    //Act
    let response = client
        .get(format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

// #[test]
// fn dummy_fail() {
//     let result: Result<&str, &str> = Err("This is a dummy failure");
//     claim::assert_ok!(result);
// }
//
