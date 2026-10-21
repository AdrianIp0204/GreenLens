// Development preparation guard only. This is not a held-handle filesystem sandbox.
import fs from 'node:fs';
import path from 'node:path';

export function validateNewExternalDestination(destination) {
  if (typeof destination !== 'string' || !destination.trim())
    throw new Error('A new external destination is required');
  if (!process.env.LOCALAPPDATA?.trim())
    throw new Error('LOCALAPPDATA is required for external preparation');
  const target = path.resolve(destination);
  const external = path.resolve(process.env.LOCALAPPDATA, 'GreenLens');
  if (!target.toLowerCase().startsWith(external.toLowerCase() + path.sep))
    throw new Error('Destination must be a new directory under LOCALAPPDATA/GreenLens');
  // lstat does not follow a dangling junction. existsSync would miss it.
  if (fs.lstatSync(target, {throwIfNoEntry:false}))
    throw new Error('Destination exists: preserve it; use a new preparation directory');
  // Missing intermediate parents do not establish that higher ancestors are safe.
  for (let parent = path.dirname(target); ; parent = path.dirname(parent)) {
    if (fs.lstatSync(parent, {throwIfNoEntry:false})?.isSymbolicLink())
      throw new Error('Reparse/symlink destination ancestry is unsupported');
    if (path.dirname(parent) === parent) break;
  }
  return target;
}
