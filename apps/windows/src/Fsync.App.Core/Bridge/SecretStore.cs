using System.Security.Cryptography;
using System.Text.Json;
using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>
/// Device secret, history key (32 random bytes each, created on first use), session tokens and
/// the last email, in one DPAPI-protected file (CurrentUser scope). Core never sees the file.
/// Writes are atomic and serialized: token refreshes arrive on the core callback thread.
/// </summary>
public sealed partial class SecretStore
{
    private static readonly byte[] Entropy = "fsync-secrets-v1"u8.ToArray();
    private readonly string _path;
    private readonly object _lock = new();
    private Data _data;

    private sealed class Data
    {
        public byte[]? DeviceSecret { get; set; }
        public byte[]? HistoryKey { get; set; }
        public string? Access { get; set; }
        public string? Refresh { get; set; }
        public string? AccountId { get; set; }
        public string? Email { get; set; }
        public PendingRevoke? PendingRevoke { get; set; }
    }

    public SecretStore(string path)
    {
        _path = path;
        _data = Load(path);
        var changed = false;
        if (_data.DeviceSecret is not { Length: 32 }) { _data.DeviceSecret = RandomNumberGenerator.GetBytes(32); changed = true; }
        if (_data.HistoryKey is not { Length: 32 }) { _data.HistoryKey = RandomNumberGenerator.GetBytes(32); changed = true; }
        if (changed) Save();
    }

    public byte[] DeviceSecret { get { lock (_lock) return (byte[])_data.DeviceSecret!.Clone(); } }
    public byte[] HistoryKey { get { lock (_lock) return (byte[])_data.HistoryKey!.Clone(); } }
    public string? Email { get { lock (_lock) return _data.Email; } }

    public SessionTokens? Session
    {
        get
        {
            lock (_lock)
                return _data is { Access: { } a, Refresh: { } r, AccountId: { } id } ? new SessionTokens(a, r, id) : null;
        }
    }

    public void SaveSession(SessionTokens tokens, string? email = null)
    {
        lock (_lock)
        {
            (_data.Access, _data.Refresh, _data.AccountId) = (tokens.access, tokens.refresh, tokens.accountId);
            if (email is not null) _data.Email = email;
            Save();
        }
    }

    public void ClearSession()
    {
        lock (_lock)
        {
            (_data.Access, _data.Refresh, _data.AccountId) = (null, null, null);
            Save();
        }
    }

    private static Data Load(string path)
    {
        if (!File.Exists(path)) return new Data();
        var plain = ProtectedData.Unprotect(File.ReadAllBytes(path), Entropy, DataProtectionScope.CurrentUser);
        try { return JsonSerializer.Deserialize<Data>(plain) ?? new Data(); }
        finally { CryptographicOperations.ZeroMemory(plain); }
    }

    private void Save()
    {
        var plain = JsonSerializer.SerializeToUtf8Bytes(_data);
        try
        {
            var sealedBytes = ProtectedData.Protect(plain, Entropy, DataProtectionScope.CurrentUser);
            Directory.CreateDirectory(Path.GetDirectoryName(_path)!);
            var tmp = _path + ".tmp";
            File.WriteAllBytes(tmp, sealedBytes);
            File.Move(tmp, _path, overwrite: true);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(plain);
        }
    }
}
