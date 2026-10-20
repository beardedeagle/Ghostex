import type { GxserverSessionLifecycleParams } from './gxserver-protocol-sessions';
import type { GxserverAgentActivityState } from './gxserver-protocol-session-runtime';
import type { GxserverSessionTitleProjection } from './gxserver-protocol-presentation';
import type { GxserverSessionDomainState } from './gxserver-protocol-domain';

export interface GxserverAgentSettings {
  agentAcceptAllEnabled: boolean;
  defaultPromptAgentId: string;
}

export interface GxserverReadAgentSettingsResult {
  isPersisted: boolean;
  settings: GxserverAgentSettings;
}

export interface GxserverUpdateAgentSettingsParams {
  agentAcceptAllEnabled?: boolean;
  defaultPromptAgentId?: string;
}

export type GxserverAgentSkillSourceKind = 'global' | 'pluginCache' | 'repository';

export interface GxserverAgentSkillLocation {
  directoryPath: string;
  providers: readonly string[];
  rootPath: string;
  skillFilePath: string;
  sourceKind: GxserverAgentSkillSourceKind;
}

export interface GxserverAgentSkillStatusRow {
  installed: boolean;
  locations: readonly GxserverAgentSkillLocation[];
  skillName: string;
}

export interface GxserverReadAgentSkillStatusParams {
  repositoryPaths?: readonly string[];
  skillNames?: readonly string[];
}

export interface GxserverReadAgentSkillStatusResult {
  generatedAt: string;
  homeDir: string;
  roots: readonly GxserverAgentSkillDiscoveryRoot[];
  skills: readonly GxserverAgentSkillStatusRow[];
  type: 'agentSkillStatus';
}

export interface GxserverAgentSkillDiscoveryRoot {
  path: string;
  providers: readonly string[];
  sourceKind: GxserverAgentSkillSourceKind;
}

export interface GxserverInstallAgentSkillsParams extends GxserverReadAgentSkillStatusParams {
  agentIds?: readonly string[];
  packageSource?: string;
}

export interface GxserverInstallAgentSkillsResult extends GxserverReadAgentSkillStatusResult {
  installCommand: readonly string[];
  packageSource: string;
  stderr: string;
  stdout: string;
}

export type GxserverAgentHookStatus = 'cliMissing' | 'installed' | 'missing' | 'updateRequired';

export interface GxserverAgentHookStatusRow {
  agentId: string;
  cliCommand: string;
  cliInstalled: boolean;
  detail: string;
  hookInstalled: boolean;
  paths: readonly string[];
  status: GxserverAgentHookStatus;
}

export interface GxserverReadAgentHookStatusParams {
  agentIds?: readonly string[];
  autoUpgradeInstalled?: boolean;
}

export interface GxserverReadAgentHookStatusResult {
  agents: readonly GxserverAgentHookStatusRow[];
  autoUpgradedPaths?: readonly string[];
  generatedAt: string;
  hookStateDirectory: string;
  notifyHookPath: string;
  type: 'agentHookStatus';
}

export interface GxserverInstallAgentHooksParams {
  agentIds?: readonly string[];
}

export interface GxserverInstallAgentHooksResult extends GxserverReadAgentHookStatusResult {
  installedPaths: readonly string[];
}

export interface GxserverUninstallAgentHooksResult extends GxserverReadAgentHookStatusResult {
  removedPaths: readonly string[];
}

export interface GxserverIngestAgentHookEventParams extends GxserverSessionLifecycleParams {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  eventName?: string;
  firstUserMessage?: string;
  rawEventName?: string;
  status?: GxserverAgentActivityState['activity'];
  statusUpdatedAt?: string;
  title?: string;
}

export interface GxserverIngestAgentHookEventResult {
  activity?: GxserverAgentActivityState;
  changed: boolean;
  enteredAttention: boolean;
  previousActivity?: GxserverAgentActivityState['activity'];
  projection: GxserverSessionTitleProjection;
  reason: string;
  session: GxserverSessionDomainState;
}
