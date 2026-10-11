import type { Model, ValidatedModel, Vec3 } from '../types';

export type ViewId = 'perspective' | 'top' | 'front' | 'right';
export type DisplayMode = 'wireframe' | 'shaded' | 'analysis';
export type LayerId = 'room' | 'window' | 'overhang' | 'obstruction' | 'sensors' | 'analysis';
export type ToolId = 'select' | 'move' | 'resize' | 'rotate';
export type SelectionMode = 'replace' | 'add' | 'remove';
export interface LayerState { visible: boolean; locked: boolean; color: string }
export interface ObjectDisplayState { visible?: boolean; locked?: boolean; color?: string }
export type LayerStates = Record<LayerId, LayerState>;
export interface CameraState { position: Vec3; target: Vec3; up: Vec3; zoom: number }
export interface ViewportDisplayState {
  schema_version: 'greenlens.display.v0.2';
  active_view: ViewId;
  maximized_view: ViewId | null;
  mode: DisplayMode;
  cutaway: boolean;
  cameras: Partial<Record<ViewId, CameraState>>;
  layers: LayerStates;
  objects: Record<string, ObjectDisplayState>;
  grid_visible: boolean;
  grid_snap: boolean;
  object_snap: boolean;
  ortho: boolean;
  gumball: boolean;
}
export interface ViewCommand { sequence: number; kind: 'extents' | 'selected' | 'four' | 'maximize'; view?: ViewId }
export interface PointerStatus { world: Vec3 | null; snap: string | null; view: ViewId; message?: string }
export interface SemanticObject { id: string; label: string; layer: LayerId; vertices: Vec3[] }
export interface NumericHandleRequest { entity: string; parameter: string; operation: 'move' | 'resize' | 'rotate'; label: string; value: number; unit: 'm' | 'deg' }
export interface SceneViewProps {
  documentId: string;
  validated: ValidatedModel | null;
  lux: number[] | null;
  resultLoading?: boolean;
  scale: number;
  selection: string[];
  selectionMode?: SelectionMode;
  display: ViewportDisplayState;
  tool: ToolId;
  disabled?: boolean;
  viewCommand?: ViewCommand;
  onSelectionChange: (ids: string[]) => void;
  onSelectionModeChange?: (mode: SelectionMode) => void;
  onDisplayChange: (display: ViewportDisplayState) => void;
  onCommitCandidate: (model: Model, label: string) => Promise<boolean>;
  onNumericHandle?: (request: NumericHandleRequest) => void;
  onPreviewCandidate?: (model: Model | null, label?: string) => void;
  onPointerStatus?: (status: PointerStatus) => void;
  onInteractionError?: (message: string) => void;
}
export const DEFAULT_LAYERS: LayerStates = {
  room: { visible: true, locked: false, color: '#c8ced0' },
  window: { visible: true, locked: false, color: '#72adb5' },
  overhang: { visible: true, locked: false, color: '#9ba895' },
  obstruction: { visible: true, locked: false, color: '#adb3ba' },
  sensors: { visible: true, locked: false, color: '#488e7b' },
  analysis: { visible: true, locked: true, color: '#80baaa' },
};
export function defaultDisplay(): ViewportDisplayState {
  return { schema_version: 'greenlens.display.v0.2', active_view: 'perspective', maximized_view: null, mode: 'shaded', cutaway: true, cameras: {}, layers: structuredClone(DEFAULT_LAYERS), objects: {}, grid_visible: true, grid_snap: false, object_snap: true, ortho: false, gumball: true };
}
