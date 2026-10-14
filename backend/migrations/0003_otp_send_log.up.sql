-- One row per OTP email send attempt (unix ms): backs the global rolling-24h send cap, which must
-- survive restarts and hold across instances (Gmail SMTP allows roughly 500 recipients a day).
CREATE TABLE otp_send_log (
    id      bigserial PRIMARY KEY,
    sent_at bigint NOT NULL
);
CREATE INDEX otp_send_log_sent_at_idx ON otp_send_log (sent_at);
