use std::collections::HashSet;

use reqwest::header::HeaderValue;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act - Part 1 - Try to login
    let login_body = serde_json::json!({
        "username": "random_user",
        "password": "random_password"
    });
    let response = app.post_login(&login_body).await;

    // Assert that the response is a redirect to the login page, which indicates that the login attempt was unsuccessful and an error flash message should be set.
    assert_is_redirect_to(&response, "/login");

    // Act - Part 2 - Follow the redirect to the login page and verify that the error message is displayed in the HTML response. This verifies that the flash message was properly set and rendered on the login page after a failed login attempt.
    let html_page = app.get_login_html().await;
    assert!(
        html_page.contains("<p><i><b><font color=\"red\">Authentication failed</font></b></i></p>"),
        "Expected the HTML page to contain the error message 'Authentication failed' in a red font, but it was not found. Actual HTML page: {}",
        html_page
    );

    // Act - Part 3 - Follow the redirect to the login page again and verify that the error message is no longer displayed in the HTML response. This verifies that the flash message is cleared after being displayed once, ensuring that it does not persist across multiple requests.
    let html_page = app.get_login_html().await;
    assert!(
        !html_page.contains("<p><i><b><font color=\"red\">Authentication failed</font></b></i></p>"),
        "Expected the error message 'Authentication failed' to be cleared from the HTML page after being displayed once, but it was still found. Actual HTML page: {}",
        html_page
    );
}
