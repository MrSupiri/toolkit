use super::model::{FCMSchedule, UpdateSchedule};
use super::utils::{decode_cron, extract_claims};
use crate::utils::{ApiTags, MyResponse, ResponseObject};
use chrono::Utc;
use poem::{web::Data, Request};
use poem_openapi::param::Path;
use poem_openapi::{payload::Json, OpenApi};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct FirebaseMessaging {
    pub projects: Vec<String>,
}

#[OpenApi(
    prefix_path = "/fcm/",
    request_header(
        name = "firebase-auth",
        ty = "String",
        description = "Bearer token generated from firebase project (example: <code>Bearer {token}</code>)"
    ),
    tag = "ApiTags::FirebaseMessaging"
)]
impl FirebaseMessaging {
    // create new instance
    pub fn new(projects: Vec<String>) -> Self {
        Self { projects }
    }

    // create schedule
    #[oai(path = "/", method = "post", operation_id = "fcm::create_schedule")]
    async fn create_schedule(
        &self,
        req: &Request,
        pool: Data<&SqlitePool>,
        payload: Json<FCMSchedule>,
    ) -> MyResponse<FCMSchedule> {
        // extract user id from token
        let data = match extract_claims(req.header("firebase-auth")) {
            Ok(data) => data,
            Err(e) => {
                return ResponseObject::unauthorized(e);
            }
        };

        let fb_user_id = data.user_id;
        let fb_project_id = data.aud;

        if !self.projects.contains(&fb_project_id) {
            return ResponseObject::unauthorized("Invalid project id");
        }

        // validate payload
        match payload.payload {
            Value::Object(_) => {}
            _ => {
                return ResponseObject::bad_request("Invalid payload");
            }
        }

        let next_execution = match decode_cron(&payload.cron_pattern.as_ref()) {
            Ok(next) => next,
            Err(e) => {
                return ResponseObject::bad_request(e);
            }
        };

        let current_time = Utc::now().naive_local();

        let result = sqlx::query!(
            "INSERT INTO fcm_schedule (
                name, fb_user_id, push_token, fb_project_id, cron_pattern, payload, last_execution, next_execution, created_at, updated_at
            ) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            payload.name,
            fb_user_id,
            payload.push_token,
            fb_project_id,
            payload.cron_pattern,
            payload.payload,
            current_time,
            next_execution,
            current_time,
            current_time
        ).execute(pool.0).await;

        let result = match result {
            Ok(result) => result.last_insert_rowid(),
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        let schedule = sqlx::query_as!(
            FCMSchedule,
            "SELECT * FROM fcm_schedule WHERE id = ?",
            result
        )
        .fetch_one(pool.0)
        .await;

        let schedule = match schedule {
            Ok(schedule) => schedule,
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        ResponseObject::created(schedule)
    }

    // find all schedules for the user
    #[oai(path = "/", method = "get", operation_id = "fcm::find_all_schedules")]
    async fn find_all_schedules(
        &self,
        req: &Request,
        pool: Data<&SqlitePool>,
    ) -> MyResponse<Vec<FCMSchedule>> {
        // extract user id from token
        let data = match extract_claims(req.header("firebase-auth")) {
            Ok(data) => data,
            Err(e) => {
                return ResponseObject::unauthorized(e);
            }
        };

        let fb_user_id = data.user_id;

        let schedules = sqlx::query_as!(
            FCMSchedule,
            "SELECT * FROM fcm_schedule WHERE fb_user_id = ?",
            fb_user_id
        )
        .fetch_all(pool.0)
        .await;

        let schedules = match schedules {
            Ok(schedules) => schedules,
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        ResponseObject::ok(schedules)
    }

    // Delete schedule by id (only if it belongs to the user)
    #[oai(
        path = "/:id",
        method = "delete",
        operation_id = "fcm::delete_schedule"
    )]
    async fn delete_schedule(
        &self,
        req: &Request,
        pool: Data<&SqlitePool>,
        id: Path<i64>,
    ) -> MyResponse<FCMSchedule> {
        // extract user id from token
        let data = match extract_claims(req.header("firebase-auth")) {
            Ok(data) => data,
            Err(e) => {
                return ResponseObject::unauthorized(e);
            }
        };

        let fb_user_id = data.user_id;

        let schedule = sqlx::query_as!(
            FCMSchedule,
            "SELECT * FROM fcm_schedule WHERE id = ? AND fb_user_id = ?",
            id.0,
            fb_user_id
        )
        .fetch_one(pool.0)
        .await;

        let schedule = match schedule {
            Ok(schedule) => schedule,
            Err(_) => {
                return ResponseObject::not_found("Schedule not found");
            }
        };

        let result = sqlx::query!(
            "DELETE FROM fcm_schedule WHERE id = ? AND fb_user_id = ?",
            id.0,
            fb_user_id
        )
        .execute(pool.0)
        .await;

        let result = match result {
            Ok(result) => result,
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        if result.rows_affected() == 0 {
            return ResponseObject::not_found("Schedule not found");
        }

        ResponseObject::ok(schedule)
    }

    // Update schedule by id (only if it belongs to the user)
    #[oai(path = "/:id", method = "put", operation_id = "fcm::update_schedule")]
    async fn update_schedule(
        &self,
        req: &Request,
        pool: Data<&SqlitePool>,
        id: Path<i64>,
        payload: Json<UpdateSchedule>,
    ) -> MyResponse<FCMSchedule> {
        // extract user id from token
        let data = match extract_claims(req.header("firebase-auth")) {
            Ok(data) => data,
            Err(e) => {
                return ResponseObject::unauthorized(e);
            }
        };

        let fb_user_id = data.user_id;

        let schedule = sqlx::query_as!(
            FCMSchedule,
            "SELECT * FROM fcm_schedule WHERE id = ? AND fb_user_id = ?",
            id.0,
            fb_user_id
        )
        .fetch_one(pool.0)
        .await;

        let _ = match schedule {
            Ok(schedule) => schedule,
            Err(_) => {
                return ResponseObject::not_found("Schedule not found");
            }
        };

        match payload.payload {
            Value::Object(_) => {}
            _ => {
                return ResponseObject::bad_request("Invalid payload");
            }
        }

        let next_execution = match decode_cron(&payload.cron_pattern) {
            Ok(next) => next,
            Err(e) => {
                return ResponseObject::bad_request(e);
            }
        };

        let current_time = Utc::now().naive_local();

        let result = sqlx::query!(
            "UPDATE fcm_schedule SET name = ?, push_token = ?, cron_pattern = ?, payload = ?, next_execution = ?, updated_at = ? WHERE id = ? AND fb_user_id = ?",
            payload.name,
            payload.push_token,
            payload.cron_pattern,
            payload.payload,
            next_execution,
            current_time,
            id.0,
            fb_user_id
        )
        .execute(pool.0)
        .await;

        let result = match result {
            Ok(result) => result,
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        if result.rows_affected() == 0 {
            return ResponseObject::not_found("Schedule not found");
        }

        let schedule = sqlx::query_as!(
            FCMSchedule,
            "SELECT * FROM fcm_schedule WHERE id = ? AND fb_user_id = ?",
            id.0,
            fb_user_id
        )
        .fetch_one(pool.0)
        .await;

        let schedule = match schedule {
            Ok(schedule) => schedule,
            Err(e) => {
                return ResponseObject::internal_server_error(e);
            }
        };

        ResponseObject::ok(schedule)
    }
}
