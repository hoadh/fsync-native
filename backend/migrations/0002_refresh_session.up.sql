-- auth_time: unix ms of the OTP login that created the family (carried through rotation);
-- the family dies 90 d after it. rotated_at: set when a token is consumed by rotation
-- (distinguishes a benign retry from theft; logout/family revocation leave it NULL).
ALTER TABLE refresh_tokens
    ADD COLUMN auth_time  bigint NOT NULL DEFAULT 0,
    ADD COLUMN rotated_at bigint;
