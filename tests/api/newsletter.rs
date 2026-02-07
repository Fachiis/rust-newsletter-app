use actix_web::http;
use uuid::Uuid;
use wiremock::{
    matchers::{any, method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::{spawn_app, ConfirmationLinks, TestApp};

/// Helper function to create an unconfirmed subscriber
async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=zasha%20felixo&email=felixo%40gmail.com";

    // Setup mock server to expect a request to Postmark API. We intercept it to ensure that our application is sending the email.
    // A mock guard is used to limit the scope of the mock to this function only. While the mock only expects one request, we don't want it to interfere with other tests.
    let _mock_guard = Mock::given(path("/email"))
        .and(method(http::Method::POST))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    // Extract the confirmation links from the intercepted email request from Postmark
    // to retrieve the confirmation links and return them
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    app.get_confirmation_links(email_request).await
}

async fn create_confirmed_subscriber(app: &TestApp) {
    let confirmation_links = create_unconfirmed_subscriber(app).await;

    // Simulate the subscriber clicking the confirmation link
    reqwest::get(confirmation_links.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[tokio::test]
async fn newsletter_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    // Create an application state with an unconfirmed subscriber
    create_unconfirmed_subscriber(&app).await;

    let _ = Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .named("Newsletter is not delivered to unconfirmed subscribers")
        .expect(0)
        .mount_as_scoped(&app.email_server)
        .await;

    // Act
    let newsletter_request_body = serde_json::json!({
    "title": "Newsletter title",
    "content": {
        "text": "Newsletter text content",
        "html": "<p>Newsletter HTML content</p>"
    }
    });
    let response = app.post_newsletters(newsletter_request_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    // Mock expectations are verified when the mock goes out of scope. This is where we verify that no email was sent.
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    // Create an application state with a confirmed subscriber
    create_confirmed_subscriber(&app).await;

    // Setup mock server to expect a request to Postmark API. We intercept it to ensure that our application is sending the email.
    // The difference between mock_guard and _mock is that the latter lives until the end of the function scope.
    let _mock = Mock::given(path("/email"))
        .and(method(http::Method::POST))
        .respond_with(ResponseTemplate::new(200))
        .named("Newsletter is delivered to confirmed subscribers")
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    let newsletter_request_body = serde_json::json!({
    "title": "Newsletter title",
    "content": {
        "text": "Newsletter text content",
        "html": "<p>Newsletter HTML content</p>"
    }
    });
    let response = app.post_newsletters(newsletter_request_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    // Mock expectations are verified when the mock goes out of scope. This is where we verify that an email was sent.
}

#[tokio::test]
async fn newsletters_return_400_for_invalid_data() {
    // Arrange
    let app = spawn_app().await;

    let test_cases = vec![
        (
            serde_json::json!({
                "content": {
                    "text": "Newsletter text content",
                    "html": "<p>Newsletter HTML content</p>"
                }
            }),
            "missing title",
        ),
        (
            serde_json::json!({
                "title": "Newsletter title",
            }),
            "missing content",
        ),
    ];

    for (invalid_body, error_message) in test_cases {
        // Act
        let response = app.post_newsletters(invalid_body).await;

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}

#[tokio::test]
async fn requests_missing_authorization_are_rejected() {
    // Arrange
    let app = spawn_app().await;

    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "text": "Newsletter text content",
            "html": "<p>Newsletter HTML content</p>"
        }
    });

    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", &app.address))
        .json(&newsletter_request_body)
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(401, response.status().as_u16());
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    );
}

#[tokio::test]
async fn non_existing_user_is_rejected() {
    // Arrange
    let start_time = std::time::Instant::now();

    let app = spawn_app().await;
    // Random credentials
    let username = Uuid::new_v4().to_string();
    let password = Uuid::new_v4().to_string();

    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", &app.address))
        .basic_auth(username, Some(password))
        .json(&serde_json::json!({
            "title": "Newsletter title",
            "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>",
        }
        }))
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(401, response.status().as_u16());
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    );

    let start_ms = start_time.elapsed().as_secs();
    println!(
        "Test non_existing_user_is_rejected takes approximately {} ms",
        start_ms
    );
}

#[tokio::test]
async fn invalid_password_is_rejected() {
    // Arrange
    let start_time = std::time::Instant::now();

    let app = spawn_app().await;
    let username = &app.test_user.username;
    // Random password
    let password = Uuid::new_v4().to_string();

    assert_ne!(app.test_user.password, password);

    let response = reqwest::Client::new()
        .post(&format!("{}/newsletters", &app.address))
        .basic_auth(username, Some(password))
        .json(&serde_json::json!({
            "title": "Newsletter title",
            "content": {
            "text": "Newsletter body as plain text",
            "html": "<p>Newsletter body as HTML</p>",
        }
        }))
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(401, response.status().as_u16());
    assert_eq!(
        r#"Basic realm="publish""#,
        response.headers()["WWW-Authenticate"]
    );

    let start_ms = start_time.elapsed().as_secs();
    println!(
        "Test invalid_password_is_rejected takes approximately {} ms",
        start_ms
    );
}
