-- All timestamps are unix milliseconds (bigint). Emails are stored lowercased.
CREATE TABLE accounts (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email      text NOT NULL UNIQUE,
    created_at bigint NOT NULL
);

CREATE TABLE otp_codes (
    email      text PRIMARY KEY,
    code_hash  bytea NOT NULL,
    expires_at bigint NOT NULL,
    attempts   integer NOT NULL DEFAULT 0
);

CREATE TABLE refresh_tokens (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    family_id  uuid NOT NULL,
    token_hash bytea NOT NULL UNIQUE,
    expires_at bigint NOT NULL,
    revoked_at bigint,
    created_at bigint NOT NULL
);
CREATE INDEX refresh_tokens_family_idx ON refresh_tokens (family_id);
CREATE INDEX refresh_tokens_account_idx ON refresh_tokens (account_id);

-- Latest list only. `epoch` increments on a trust reset (re-genesis, ADR 012).
CREATE TABLE device_lists (
    account_id uuid PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    epoch      integer NOT NULL,
    seq        bigint NOT NULL,
    signed     bytea NOT NULL,
    updated_at bigint NOT NULL
);

CREATE TABLE device_list_history (
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    epoch      integer NOT NULL,
    seq        bigint NOT NULL,
    signed     bytea NOT NULL,
    created_at bigint NOT NULL,
    PRIMARY KEY (account_id, epoch, seq)
);

-- code_hash = keyed hash over (account_id, code): unique per account.
CREATE TABLE pairing_codes (
    code_hash  bytea PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    node_id    text NOT NULL,
    relay_url  text,
    expires_at bigint NOT NULL,
    used_at    bigint
);
CREATE INDEX pairing_codes_account_node_idx ON pairing_codes (account_id, node_id);
