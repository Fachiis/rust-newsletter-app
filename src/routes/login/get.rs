use actix_web::{http::header::ContentType, HttpResponse};
use actix_web_flash_messages::{IncomingFlashMessages, Level};
use std::fmt::Write;

pub async fn login_form(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut error_html = String::new();
    for m in flash_messages.iter().filter(|m| m.level() == Level::Error) {
        writeln!(
            error_html,
            "<p><i><b><font color=\"red\">{}</font></b></i></p>",
            m.content()
        )
        .unwrap();
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"
            <!DOCTYPE html>
            <html lang="en">

                <head>
                    <meta name="viewport" content="width=device-width, initial-scale=1.0">
                    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
                    <script src="https://cdn.tailwindcss.com"></script>
                    <title>Login</title>
                </head>

                <body>
                    <h1 class="text-3xl font-bold underline text-center my-4">Login</h1>
                    <div class="flex items-center justify-center min-h-screen bg-gray-100">
                        <div class="w-full max-w-md p-8 space-y-6 bg-white rounded-lg shadow-md">
                            <div class="flex justify-center">{error_html}</div>
                            <form class="space-y-4" action="/login" method="post">
                                <div>
                                    <label for="username" class="block text-sm font-medium text-gray-700 mb-2">Username:</label>
                                    <input type="text" id="username" name="username" placeholder="Enter Username"
                                        class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition">
                                </div>
                                <div>
                                    <label for="password" class="block text-sm font-medium text-gray-700 mb-2">Password:</label>
                                    <input type="password" id="password" name="password" placeholder="Enter Password"
                                        class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition">
                                </div>
                                <button type="submit"
                                    class="w-full px-4 py-2 text-white bg-blue-600 rounded-lg hover:bg-blue-700 focus:ring-4 focus:ring-blue-300 font-medium transition">
                                    Login
                                </button>
                            </form>
                        </div>
                    </div>
                </body>

            </html>
            "#
        ))
}
