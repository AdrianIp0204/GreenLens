/** UI task lifetime: parent busy state may be caused by this task itself. */
export async function runStudyTask(context, inFlight, disabled, action, handlers) {
  if (inFlight.current || disabled.current) return false;
  const generation = context.current;
  const current = () => context.current === generation;
  const canContinue = () => current() && !disabled.current;
  inFlight.current = true;
  handlers.start();
  try { await action(current, canContinue); }
  catch (reason) { if (current()) handlers.failure(reason); }
  finally { if (current()) { inFlight.current = false; handlers.finish(); } }
  return true;
}
