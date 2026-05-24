export type NetplaySessionMode = "controllerNetplay" | "linkCable";

export type NetplayClientKind = "desktop" | "android";

export type LinkCableTransport = "relay";

export type NetplayVoiceMode =
  | "voiceActivation"
  | "pushToTalk"
  | "mutedOnJoin";

export type RoomVoiceStatus = "available" | "unavailable";

export type PlayerRole = "host" | "guest";

export type PlayerStatus =
  | "empty"
  | "connected"
  | "checkingCompatibility"
  | "compatibilityFailed"
  | "syncingState"
  | "ready"
  | "playing"
  | "paused"
  | "reconnecting"
  | "recoveryExpired"
  | "disconnected";

export type PlayerRuntimeState =
  | "empty"
  | "connected"
  | "checkingCompatibility"
  | "syncing"
  | "ready"
  | "playing"
  | "pausing"
  | "paused"
  | "reconnecting"
  | "stale"
  | "disconnected"
  | "recoveryExpired";

export type ClientRuntimeState =
  | "connected"
  | "checkingCompatibility"
  | "syncing"
  | "ready"
  | "playing"
  | "pausing"
  | "paused"
  | "reconnecting"
  | "disconnected";

export type RoomStatus =
  | "waitingForGuest"
  | "checkingCompatibility"
  | "syncingState"
  | "ready"
  | "playing"
  | "paused"
  | "recovering"
  | "closed";

export type SessionPauseReason =
  | "menu"
  | "backgrounded"
  | "system"
  | "connectionLost";

export type SessionPauseState = "pausing" | "paused";
