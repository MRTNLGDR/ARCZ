from __future__ import annotations
from dataclasses import dataclass, field
from enum import StrEnum
import ipaddress, os, socket, threading
from urllib.parse import urlparse
from .errors import ApiError

class NetworkMode(StrEnum):
    OFFLINE_STRICT = "offline_strict"
    LOCAL_LAN = "local_lan"
    IMPORT_ASSISTED = "import_assisted"

@dataclass(slots=True)
class NetworkPolicy:
    mode: NetworkMode = NetworkMode.OFFLINE_STRICT
    allow_loopback: bool = True
    local_lan_cidrs: tuple[str, ...] = ()
    import_allowlist: frozenset[str] = field(default_factory=frozenset)

    @classmethod
    def from_environment(cls) -> "NetworkPolicy":
        raw = os.environ.get("ARCZ_NETWORK_MODE", NetworkMode.OFFLINE_STRICT.value)
        try: mode = NetworkMode(raw)
        except ValueError as e: raise ApiError("NETWORK_MODE_INVALID", f"ARCZ_NETWORK_MODE inválido: {raw}", status=500) from e
        cidrs = tuple(x.strip() for x in os.environ.get("ARCZ_LOCAL_LAN_CIDRS", "").split(",") if x.strip())
        allow = frozenset(x.strip().lower() for x in os.environ.get("ARCZ_IMPORT_ALLOWLIST", "").split(",") if x.strip())
        return cls(mode=mode, local_lan_cidrs=cidrs, import_allowlist=allow)

    def allows_host(self, host: str) -> bool:
        host = host.strip("[]").lower()
        if host in {"localhost", "localhost.localdomain"}: return self.allow_loopback
        try: ip = ipaddress.ip_address(host)
        except ValueError:
            if self.mode is NetworkMode.IMPORT_ASSISTED and host in self.import_allowlist: return True
            return False
        if ip.is_loopback: return self.allow_loopback
        if self.mode is NetworkMode.OFFLINE_STRICT: return False
        if self.mode is NetworkMode.LOCAL_LAN:
            return any(ip in ipaddress.ip_network(cidr, strict=False) for cidr in self.local_lan_cidrs)
        return True if self.mode is NetworkMode.IMPORT_ASSISTED else False

    def assert_url(self, url: str) -> None:
        parsed = urlparse(url)
        if parsed.scheme not in {"http","https"}: raise ApiError("NETWORK_SCHEME_DENIED", f"Esquema não permitido: {parsed.scheme}", status=403)
        host = parsed.hostname or ""
        if not self.allows_host(host): raise ApiError("NETWORK_EGRESS_DENIED", f"Egress bloqueado para {host} em {self.mode.value}", status=403, details={"host":host,"mode":self.mode.value})

_original_connect = socket.socket.connect
_original_create_connection = socket.create_connection
_guard_lock = threading.Lock()
_guard_policy: NetworkPolicy | None = None

def install_egress_guard(policy: NetworkPolicy) -> None:
    global _guard_policy
    with _guard_lock:
        _guard_policy = policy
        if getattr(socket.socket.connect, "__arcz_guard__", False): return
        def guarded_connect(sock: socket.socket, address):
            host = address[0] if isinstance(address, tuple) else str(address)
            current = _guard_policy or policy
            if not current.allows_host(str(host)):
                raise OSError(f"ARCZ egress denied: {host} ({current.mode.value})")
            return _original_connect(sock, address)
        guarded_connect.__arcz_guard__ = True  # type: ignore[attr-defined]
        socket.socket.connect = guarded_connect  # type: ignore[assignment]
        def guarded_create_connection(address, *args, **kwargs):
            host = address[0]
            current = _guard_policy or policy
            if not current.allows_host(str(host)):
                raise OSError(f"ARCZ egress denied: {host} ({current.mode.value})")
            return _original_create_connection(address, *args, **kwargs)
        guarded_create_connection.__arcz_guard__ = True  # type: ignore[attr-defined]
        socket.create_connection = guarded_create_connection  # type: ignore[assignment]
