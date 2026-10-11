export function runStudyTask(
  context: {current: number}, inFlight: {current: boolean}, disabled: {current: boolean},
  action: (current: () => boolean, canContinue: () => boolean) => Promise<void>,
  handlers: {start: () => void; failure: (reason: unknown) => void; finish: () => void},
): Promise<boolean>;
