import type { EasyConnectCode, TailscaleCode } from './ghostex-remote-pairing';
import { GXSERVER_LOCAL_API_PORT } from './gxserver-protocol-core';
import type {
  GxserverProduct,
  GxserverProtocolVersion,
  GxserverServerId,
  GxserverProjectId,
  GxserverSessionId,
  GxserverLifecycleState,
  GxserverMinimalHealthResponse,
  GxserverListenerConfig,
  GxserverMigrationStatus,
} from './gxserver-protocol-core';

/*
CDXC:Portless 2026-06-23-00:25:
Phase 12 Portless contracts are metadata-only. Health and presentation may expose enums, counts, stable project/session ids, protocol, hostname, and port fields, while action availability remains explicit and local-Mac-only so remote gxserver payloads cannot advertise runnable privileged setup actions.

CDXC:Portless 2026-06-23-04:02:
Phase 14 adds read-only assigned domains to presentation metadata so Settings
can show persisted project/worktree hostnames separately from live route
previews. Keep the payload to stable ids and hostnames; names, paths, full
URLs, command text, process output, and runtime variables stay out.

CDXC:Portless 2026-06-23-04:28:
Phase 16 adds one metadata-only gxserver state update RPC for Portless protocol
changes, admin results, Disable, retry, and explicit service removal. Payloads
must stay enum/boolean/protocol only so setup recovery never carries paths,
commands, process output, URLs, tokens, environment values, or Portless files.
*/
export type GxserverPortlessProtocol = 'https' | 'http';
export type GxserverPortlessSetupOwnership = 'unknown' | 'missing' | 'ghostex' | 'standalone';
export type GxserverPortlessSetupStatus = 'unknown' | 'needed' | 'active' | 'failed' | 'disabled' | 'postponed';
export type GxserverPortlessRuntimeStatus = 'unknown' | 'inactive' | 'active' | 'failed';
export type GxserverPortlessPayloadSourceStatus = 'current' | 'missing' | 'unavailable';
export type GxserverPortlessAdminAction = 'install' | 'reconfigure' | 'retry' | 'remove';
export type GxserverPortlessActionUnavailableReason = 'nativeAdminBridgeRequired' | 'notRecommended';
export type GxserverPortlessRoutePreviewStatus = 'current' | 'disabled' | 'unavailable';
export type GxserverPortlessRoutePreviewKind = 'primary' | 'additional';
export type GxserverPortlessAssignedDomainKind = 'project' | 'worktree';
export type GxserverPortlessStateUpdateParams =
  | {
      enabled: boolean;
      kind: 'setEnabled';
    }
  | {
      kind: 'setProtocol';
      protocol: GxserverPortlessProtocol;
    }
  | {
      action: GxserverPortlessAdminAction;
      kind: 'recordAdminResult';
      ok: boolean;
      protocol?: GxserverPortlessProtocol;
    };

export interface GxserverPortlessStateUpdateResult {
  presentation: GxserverPortlessPresentation;
  status: GxserverPortlessStatus;
}

export interface GxserverPortlessAdminActionAvailability {
  available: boolean;
  localMacOnly: true;
  recommended: boolean;
  unavailableReason?: GxserverPortlessActionUnavailableReason;
}

export type GxserverPortlessAdminActionSet = Record<
  GxserverPortlessAdminAction,
  GxserverPortlessAdminActionAvailability
>;

export interface GxserverPortlessStatus {
  actions: GxserverPortlessAdminActionSet;
  enabled: boolean;
  protocol: GxserverPortlessProtocol;
  runtimeStatus: GxserverPortlessRuntimeStatus;
  setupOwnership: GxserverPortlessSetupOwnership;
  setupStatus: GxserverPortlessSetupStatus;
  sourceStatus: GxserverPortlessPayloadSourceStatus;
  updatedAt?: string;
}

export interface GxserverPortlessRoutePreview {
  hostname: string;
  kind: GxserverPortlessRoutePreviewKind;
  port: number;
  projectId: GxserverProjectId;
  protocol: GxserverPortlessProtocol;
  sessionId: GxserverSessionId;
}

export interface GxserverPortlessAssignedDomain {
  hostname: string;
  kind: GxserverPortlessAssignedDomainKind;
  parentProjectId?: GxserverProjectId;
  projectId: GxserverProjectId;
}

export interface GxserverPortlessPresentation {
  assignedDomains: readonly GxserverPortlessAssignedDomain[];
  liveListenerCount: number;
  routePreviewStatus: GxserverPortlessRoutePreviewStatus;
  routePreviews: readonly GxserverPortlessRoutePreview[];
  status: GxserverPortlessStatus;
}

/**
 * tailcat is the control-plane-free remote-access sidecar gxserver supervises.
 * `token` is the address blob clients dial; it is derived from the daemon-owned
 * server key at runtime and is null until the running sidecar has published it.
 */
export interface GxserverTailcatStatus {
  installing?: boolean;
  installProgress?: string | null;
  installError?: string | null;
  enabled: boolean;
  running: boolean;
  binaryFound: boolean;
  binaryPath: string | null;
  binaryVersion: string | null;
  token: string | null;
  ports: readonly number[];
  allowedClientKeys: readonly string[];
  lastError: string | null;
}

export interface GxserverTailcatStatusResult {
  status: GxserverTailcatStatus;
}

export type GxserverTailcatStateUpdate =
  | { kind: 'setEnabled'; enabled: boolean }
  | { kind: 'setPorts'; ports: readonly number[] }
  | { kind: 'setAllowedClientKeys'; allowedClientKeys: readonly string[] };

/*
CDXC:RemotePairing 2026-09-03:
Remote access status (SSH access + Tailscale + this computer's identity) and
the pairing codes shown in Settings → Remote. The structured pairing payloads
come from `/api/remotePairingCode`, never from the raw sidecar token, so the
QR can carry the user, ports, and the one-time pairing secret.
*/
export type GxserverRemoteAccessPlatform = 'macos' | 'windows' | 'linux';

export interface GxserverRemoteSshAccessStatus {
  enabled: boolean;
  /** The SSH port that was probed (the one Easy Connect serves). */
  port: number;
  /** ISO timestamp of the probe. */
  checkedAt: string;
  /** Per-OS service detail (launchd / systemd / Windows service state). */
  detail: string | null;
}

export interface GxserverRemoteTailscaleStatus {
  installed: boolean;
  running: boolean;
  /** Signed-in login name, or the tailnet name when the login is unknown. */
  account: string | null;
  /** MagicDNS name without the trailing dot. */
  magicDnsName: string | null;
  /** First Tailscale IP (100.x.y.z). */
  ip: string | null;
  /** Whether this node runs Tailscale SSH; null when Tailscale is not running. */
  sshEnabled: boolean | null;
}

export interface GxserverRemoteAccessStatus {
  computerName: string;
  username: string;
  platform: GxserverRemoteAccessPlatform;
  ssh: GxserverRemoteSshAccessStatus;
  tailscale: GxserverRemoteTailscaleStatus;
}

export type GxserverEnableSshAccessOutcome = 'enabled' | 'cancelled' | 'failed';

export interface GxserverEnableSshAccessResult {
  outcome: GxserverEnableSshAccessOutcome;
  message: string | null;
  /** SSH status re-read after the enable attempt. */
  ssh: GxserverRemoteSshAccessStatus;
}

export interface GxserverRemotePairingCodeResult {
  /** Present only while Easy Connect is running with a published address. */
  easyConnect?: { payload: string; code: EasyConnectCode };
  /** Present only while Tailscale is running with a MagicDNS name or IP. */
  tailscale?: { payload: string; code: TailscaleCode };
}

export interface GxserverPairedDevice {
  id: string;
  name: string;
  platform: string;
  pairedAt: string;
  lastSeenAt: string | null;
  sshKeyFingerprint: string;
}

// M2 (paired devices + pairing registration): served by
// `server/src/remote_access/{paired_devices,pair_device}.rs`.
export interface GxserverPairedDevicesResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverRemovePairedDeviceParams {
  deviceId: string;
}

export interface GxserverRemovePairedDeviceResult {
  devices: readonly GxserverPairedDevice[];
}

export interface GxserverPairDeviceParams {
  secret: string;
  deviceName: string;
  platform: string;
  sshPublicKey: string;
  tailcatClientKey?: string;
}

export interface GxserverPairDeviceResult {
  deviceId: string;
  user: string;
  computerName: string;
}

export interface GxserverPairedDeviceSeenParams {
  deviceId: string;
}

export interface GxserverPairedDeviceSeenResult {
  device: GxserverPairedDevice;
}

export interface GxserverServerHealthResponse extends GxserverMinimalHealthResponse {
  buildIdentity: string;
  capabilities: readonly string[];
  listeners: {
    local: GxserverListenerConfig;
    remote: GxserverListenerConfig;
  };
  migration: GxserverMigrationStatus;
  pid: number;
  portless?: GxserverPortlessStatus;
  port: typeof GXSERVER_LOCAL_API_PORT;
  serverId: GxserverServerId;
  startedAt: string;
  tools: readonly GxserverToolCapabilityStatus[];
}

export type GxserverToolName = 'zmx' | 'bd';
export type GxserverToolAvailability = 'available' | 'missing' | 'notExecutable' | 'unsupported';
export type GxserverToolResolutionSource = 'devSubmodule' | 'appResource' | 'gxserverBundle';

export interface GxserverToolCapabilityStatus {
  availability: GxserverToolAvailability;
  candidatePaths?: readonly string[];
  capability: 'zmxLifecycle' | 'beadsProjectBoard' | 'deferred';
  executablePath?: string;
  guidance?: string;
  message: string;
  source?: GxserverToolResolutionSource;
  tool: GxserverToolName;
}

export interface GxserverRuntimeMetadata {
  buildIdentity: string;
  pid: number;
  port: typeof GXSERVER_LOCAL_API_PORT;
  protocolVersion: GxserverProtocolVersion;
  serverId: GxserverServerId;
  startedAt: string;
  version: string;
}

export interface GxserverStatusResponse {
  health?: GxserverServerHealthResponse;
  metadata?: GxserverRuntimeMetadata;
  message: string;
  ok: boolean;
  product: GxserverProduct;
  state: GxserverLifecycleState;
}

export interface GxserverProtocolMismatch {
  actualProtocolVersion: unknown;
  expectedProtocolVersion: GxserverProtocolVersion;
  message: string;
  product: GxserverProduct;
}
