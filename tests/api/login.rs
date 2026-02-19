use std::collections::HashSet;

use reqwest::header::HeaderValue;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let login_body = serde_json::json!({
        "username": "random_user",
        "password": "random_password"
    });
    let response = app.post_login(&login_body).await;

    let html_page = app.get_login_html().await;

    let cookies: HashSet<_> = response
        .headers()
        .get_all("Set-Cookie")
        .into_iter()
        .collect(); // Collect all the Set-Cookie headers into a HashSet to ensure that we have unique cookies. This is important because we want to check if the flash cookie is set, and we don't want to have duplicate cookies in our collection.

    // Assert
    assert!(
        cookies.contains(&HeaderValue::from_str("_flash=Authentication failed").unwrap()),
        "Expected a flash cookie with the value '_flash=Authentication failed' to be set, but it was not found in the response cookies. Actual cookies: {:?}",
        cookies
    );

    // Assert that the response is a redirect to the login page, which indicates that the login attempt was unsuccessful and an error flash message should be set.
    assert_is_redirect_to(&response, "/login");

    assert!(
        html_page.contains("<p><i><b><font color=\"red\">Authentication failed</font></b></i></p>"),
        "Expected the HTML page to contain the error message 'Authentication failed' in a red font, but it was not found. Actual HTML page: {}",
        html_page
    );
}
