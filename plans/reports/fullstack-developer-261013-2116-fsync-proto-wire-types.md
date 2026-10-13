# fsync-proto wire types: public API

Crate `core/fsync-proto`. No I/O. ed25519-dalek 3.0, postcard 1.1, blake3 1, serde_json (DTO tests + JSON).
All items re-exported at root unless noted (module paths `api_dto`, `presence`, `frame`, `pairing`, `clip`, `file`).

## Identity
- `NodeId(pub [u8;32])`: Display/FromStr = 64 lowercase hex (same as iroh; FromStr accepts upper too). Serde: hex string in JSON, raw 32 bytes in postcard.
  `from_signing_key(&SigningKey)`, `From<&SigningKey>`, `verifying_key() -> Result<VerifyingKey>`, `as_bytes()`, `to_base32()`/`from_base32()` (lowercase, no pad; QR only).
- `Sig = FixedBytes<64>`, `Nonce = FixedBytes<32>` (`.0` array, `From<[u8;N]>`): base64 (std) in JSON, raw bytes in postcard.
- `sig::{sign_domain(key, domain, body) -> Sig, verify_domain(&NodeId, domain, body, &Sig) -> Result<()>}` (verify_strict).

## Device list
- `DeviceList{v,account_id:Uuid,seq,prev_hash:[u8;32],devices:Vec<DeviceEntry>,signer:NodeId,created_ms}`; `DeviceEntry{node_id,name,platform,added_seq}`; `Platform{Mac,Win,Cli}`.
- `DeviceList::genesis(account_id, signer:NodeId, name, platform, created_ms) -> DeviceList`
- `DeviceList::next_add(prev:&SignedDeviceList, signer:NodeId, node_id:NodeId, name, platform, created_ms) -> Result<DeviceList>` (seq+1, prev_hash, added_seq=seq; DuplicateDevice)
- `DeviceList::next_remove(prev, signer:NodeId, &node_id, created_ms) -> Result<DeviceList>` (DeviceNotFound / EmptyList if last)
- `DeviceList::contains(&NodeId)`, `to_bytes()`
- `SignedDeviceList{body:Vec<u8>, sig:Sig}`: `sign(&SigningKey,&DeviceList)` (also free fn `sign`; SignerMismatch if key != list.signer), `decode() -> DeviceList`, `to_bytes()/from_bytes()`, `hash() -> [u8;32]` (blake3 of postcard(SignedDeviceList)).
- Domain const `device_list::DEVLIST_DOMAIN = b"fsync-devlist-v1"`.

## Verify (`verify::*`)
- `verify_genesis(&Signed) -> Result<DeviceList>`
- `verify_next(new:&Signed, pinned:&Signed) -> Result<Outcome>`; `Outcome::{Unchanged, Advanced(DeviceList)}`
- `verify_chain(pinned, &[Signed]) -> Result<Outcome>` (consecutive lists; identical-to-head skipped)
- `verify_chain_from_genesis(&[Signed]) -> Result<DeviceList>` (fresh device after pairing; no pin yet)
- Check order in verify_next: identical->Unchanged; WrongAccount; seq 0 => valid genesis ? TrustReset : genesis error; seq<=pinned => Rollback; seq>pinned+1 => Gap; SignerNotTrusted; BadSignature; PrevHashMismatch. Shape checks (version, empty, duplicate) -> UnsupportedVersion/EmptyList/DuplicateDevice.
- `ProtoError` (thiserror, PartialEq): Codec, TrailingBytes, UnsupportedVersion, InvalidKey, BadSignature, SignerMismatch, InvalidGenesis, EmptyList, DuplicateDevice, DeviceNotFound, Rollback{got,pinned}, Gap{expected,got}, WrongAccount, SignerNotTrusted, PrevHashMismatch, TrustReset, FrameTooLarge, Truncated{needed}, InvalidNodeId, InvalidQr, InvalidOffer, InvalidPath, MisalignedOffset, InvalidBase64.
- Caveat: any seq-0 list for the same account that differs from pinned head returns TrustReset, including a replay of the OLD genesis when pinned.seq>0 (cannot tell without the root; callers should treat as warning + user confirm, then re-verify with `verify_genesis`).

## Frames (`frame`)
- `MAX_FRAME_LEN = 1 MiB + 256 KiB`, `HEADER_LEN = 4`
- `encode_frame<T:Serialize>(&T) -> Result<Vec<u8>>` (u32-LE len + postcard)
- `payload_len([u8;4]) -> Result<usize>` (check before allocating), `decode_payload<T>(&[u8])`, `decode_frame<T>(&[u8]) -> Result<(T, consumed)>` (`Truncated{needed}` = need more bytes)

## pair/1 (`pairing`)
- `PairFrame::{Hello{v,mode:PairMode,node_id}, Spake{msg:Vec<u8>}, Confirm{mac:[u8;32]}, DeviceInfo{name,platform}, SignedList{chain:Vec<SignedDeviceList>}, Abort{reason:AbortReason}, Done}`; `PairMode{Code,Qr}`; `AbortReason{BadMac,Rejected,UnsupportedVersion,Internal}`; `PAIR_VERSION=1`.
- `QrPayload{node_id, secret:[u8;16], relay:Option<String>}`: `build() -> String` = `fsync://pair?v=1&n=<base32>&s=<b64url nopad>[&r=<urlencoded>]`, `parse(&str) -> Result<Self>`. Debug redacts secret; zeroized on drop.

## clip/1, file/1
- `ALPN_CLIP/ALPN_FILE/ALPN_PAIR` (`&[u8]`).
- `ClipOffer{v,item_id:Uuid,origin:NodeId,ts_ms,kind:ClipKind,reps:Vec<RepMeta>,inline:Vec<Option<Vec<u8>>>,files:Vec<FileMeta>,preview:Option<String>,push:bool}`; `ClipKind{Text,Rich,Image,Files}`; `RepMeta{mime,size,hash}`; `FileMeta{file_id:u32,name,rel_path,size,hash,mtime_ms}`.
- `ClipOffer::validate()` (receiver-side: version, inline len/size/blake3 hash, 256 KiB inline budget, preview <=200 chars, files iff kind==Files, unique file_id, rel_path safe). `clip::validate_rel_path`. Consts `MAX_INLINE_TOTAL`, `MAX_PREVIEW_CHARS`, `CLIP_VERSION`. Note `push` field added (phase-07 "Send to..." flag).
- `FetchReq{v,item_id,target:FetchTarget{Rep(u32),File(u32)},offset}`: `FetchReq::new(..)`, `validate()` (v, offset % 16 KiB == 0; `file::CHUNK_GROUP`); `FetchResp{status:FetchStatus{Ok,NotFound,Changed,Denied},size,hash}`.

## HTTP DTOs (`api_dto`, JSON)
`OtpStart{email}`, `OtpVerify{email,code}`, `TokenPair{access,refresh}`, `OtpVerifyResponse{#[flatten] tokens, account_id}`, `Refresh{refresh}` (used for refresh + logout), `DeviceListResponse{signed(b64 std),seq}` (`new(&Signed)`, `decode()`), `PutDeviceList{signed}` (`new`, `decode`), `PairingCreate{node_id,relay_url:Option}`, `PairingCreated{code,expires_at}` (expires_at = unix ms; spec did not state unit), `PairingLookup{node_id,relay_url}`, `ApiErrorBody{code,message}` (my addition), fns `encode_signed/decode_signed`, `is_valid_pairing_code` (6 ASCII digits). Debug of tokens redacted.

## Presence (`presence`, JSON tag `type`, snake_case)
- `ClientMsg::{Auth{access,node_id,sig}, Hello{addrs:Addrs}, Heartbeat}`; `ServerMsg::{Challenge{nonce}, Peers{peers}, PeerUpdate(PeerInfo) (flattened fields), DeviceListChanged{seq}}`; `Addrs{relay_url:Option<String>,direct:Vec<SocketAddr>}`; `PeerInfo{node_id,online,addrs,last_seen_ms}`.
- `PRESENCE_DOMAIN = b"fsync-presence-v1"`, `sign_challenge(&SigningKey,&Nonce) -> Sig`, `verify_challenge(&NodeId,&Nonce,&Sig) -> Result<()>`.
- Server sends `Challenge` first on connect; no explicit AuthOk/error message defined (first `Peers` = implicit ack; close socket on failure).

## Deps added
Workspace: ed25519-dalek 3, base64 0.22, data-encoding 2, url 2, serde_json 1, zeroize 1. Crate: + blake3, uuid.

## Status
`cargo test -p fsync-proto` 30 tests pass (tests in `core/fsync-proto/tests/`), clippy -D warnings clean, fmt check clean. All src files < 200 lines.

## Review fixes (round 1) - API changes
- New modules `names` (`validate_component`, `validate_file_name`, `validate_rel_path`, `validate_device_name`; consts `MAX_COMPONENT_BYTES`=255, `MAX_REL_PATH_BYTES`=1024, `MAX_DEVICE_NAME_CHARS`=64) and `shape` (`MAX_DEVICES`=32; `DeviceList::check_shape` now pub).
- Name rules: single component, no `/ \ :`, no control chars, no trailing dot/space, no Windows reserved names (CON/PRN/AUX/NUL/COM1-9/LPT1-9, case-insens., with or without extension); device names also reject bidi/invisible chars.
- `clip::validate_rel_path` is now a re-export of `names::validate_rel_path` (stricter). `ClipOffer::validate` also checks `FileMeta.name`, caps `MAX_FILES`=10_000, `MAX_REPS`=16, `MAX_MIME_CHARS`=128 (consts in `clip`).
- `verify_genesis` / `verify_next` / `DeviceList::next_add` enforce `check_shape`; seq uses `checked_add`.
- `ProtoError`: + `SeqOverflow`, `InvalidDeviceList(&'static str)`.
- Hardening: QR secret intermediates `Zeroizing`; `encode_frame` uses `u32::try_from`.
- Tests: `names_and_caps.rs`, `shape_caps.rs`.
