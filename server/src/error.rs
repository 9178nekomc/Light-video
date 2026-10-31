use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("未登录或登录已过期")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("请求过于频繁，请稍后再试")]
    TooManyRequests(u64),
    #[error("请求体过大")]
    PayloadTooLarge,
    #[error("不支持的文件类型")]
    UnsupportedMedia,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::BadRequest(_) => "bad_request",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden(_) => "forbidden",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::TooManyRequests(_) => "rate_limited",
            AppError::PayloadTooLarge => "payload_too_large",
            AppError::UnsupportedMedia => "unsupported_media",
            AppError::Database(_) | AppError::Other(_) => "internal_error",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::UnsupportedMedia => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AppError::Database(_) | AppError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // 内部错误对外只返回通用文案，详情报日志，避免泄露表结构/SQL
        let (public_message, log_internal) = match &self {
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                ("服务器内部错误".to_string(), true)
            }
            AppError::Other(e) => {
                tracing::error!(error = %e, "internal error");
                ("服务器内部错误".to_string(), true)
            }
            other => (other.to_string(), false),
        };
        let _ = log_internal;

        let status = self.status();
        let mut body = json!({
            "error": { "code": self.code(), "message": public_message }
        });

        if let AppError::TooManyRequests(retry) = &self {
            body["error"]["retry_after"] = json!(retry);
        }

        let mut resp = (status, Json(body)).into_response();
        if matches!(status, StatusCode::TOO_MANY_REQUESTS) {
            if let AppError::TooManyRequests(retry) = &self {
                if let Ok(v) = HeaderValue::from_str(&retry.to_string()) {
                    resp.headers_mut().insert(header::RETRY_AFTER, v);
                }
            }
        }
        resp
    }
}

/// 便捷宏：`bad_request!("标题不能为空")`
#[macro_export]
macro_rules! bad_request {
    ($($arg:tt)*) => { $crate::error::AppError::BadRequest(format!($($arg)*)) };
}

#[macro_export]
macro_rules! not_found {
    ($($arg:tt)*) => { $crate::error::AppError::NotFound(format!($($arg)*)) };
}

#[macro_export]
macro_rules! forbidden {
    ($($arg:tt)*) => { $crate::error::AppError::Forbidden(format!($($arg)*)) };
}
