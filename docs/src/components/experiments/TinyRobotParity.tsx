import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { JSX } from 'preact';
import './TinyRobotParity.css';

/**
 * The recorded event format is deliberately loose: it is also used for events
 * streamed from the GPU gateway. Keeping this component independent of the
 * island runner makes the room editor usable for both kinds of result.
 */
export type TinyRobotEvent = Record<string, any>;

export type TinyRobotRecording = {
  events?: TinyRobotEvent[];
};

export type TinyRobotParityProps = {
  /** A sample.json-shaped recording. */
  recording?: TinyRobotRecording | TinyRobotEvent[];
  /** Prefer these when a live run is supplying events incrementally. */
  events?: TinyRobotEvent[];
  /** The event currently being replayed or streamed. */
  current?: TinyRobotEvent;
};

type Room = {
  walls: string;
  start: number;
  heading: number;
  key: number;
  door: number;
  exit: number;
};

type Frame = [number, number, number, boolean];
type Tool = 'wall' | 'start' | 'key' | 'door' | 'exit';
type Simulation = {
  frames: Frame[];
  solved: boolean;
  steps: number;
  visited: number;
};
type Preview = {
  room: Room;
  frames: Frame[];
  solved?: boolean;
};

const actions = ['Move forward', 'Turn left', 'Turn right', 'Turn around'];
const formatNumber = (value: unknown) =>
  typeof value === 'number' ? new Intl.NumberFormat('en-US').format(value) : '—';

const neighbor = (cell: number, direction: number) =>
  [cell - 8, cell + 1, cell + 8, cell - 1][direction & 3] & 63;

const inside = (cell: number) => (cell >> 3) > 0 && (cell >> 3) < 7 && (cell & 7) > 0 && (cell & 7) < 7;

const neighbors = (cell: number) => [0, 1, 2, 3].map((direction) => neighbor(cell, direction)).filter(inside);

const wall = (room: Room, cell: number) =>
  (BigInt(`0x${room.walls}`) & (1n << BigInt(cell))) !== 0n;

const sensors = (room: Room, cell: number, heading: number, hasKey: boolean) => {
  const isOpen = (next: number) => !wall(room, next) && (hasKey || next !== room.door);
  return [
    isOpen(neighbor(cell, heading)),
    isOpen(neighbor(cell, heading + 3)),
    isOpen(neighbor(cell, heading + 1))
  ] as const;
};

function simulate(decisions: number[], room: Room): Simulation {
  let cell = room.start;
  let heading = room.heading;
  let memory = 0;
  let hasKey = false;
  let steps = 0;
  const visited = new Set<number>();
  const frames: Frame[] = [];

  while (true) {
    visited.add(cell);
    if (cell === room.key) hasKey = true;
    frames.push([cell, heading, memory, hasKey]);
    if ((cell === room.exit && hasKey) || steps === 256) break;

    const [front, left, right] = sensors(room, cell, heading, hasKey);
    const row = Number(front) + 2 * Number(left) + 4 * Number(right) + 8 * memory;
    const decision = decisions[row] ?? 0;
    memory = decision >> 2;
    switch (decision & 3) {
      case 0:
        if (front) cell = neighbor(cell, heading);
        break;
      case 1:
        heading = (heading + 3) & 3;
        break;
      case 2:
        heading = (heading + 1) & 3;
        break;
      default:
        heading = (heading + 2) & 3;
        break;
    }
    steps += 1;
  }

  return { frames, solved: cell === room.exit && hasKey, steps, visited: visited.size };
}

function distances(room: Room, start: number, locked = false) {
  const result = Array<number>(64).fill(999);
  const queue = [start];
  result[start] = 0;
  for (let index = 0; index < queue.length; index += 1) {
    const cell = queue[index];
    for (const next of neighbors(cell)) {
      if (!wall(room, next) && !(locked && next === room.door) && result[next] === 999) {
        result[next] = result[cell] + 1;
        queue.push(next);
      }
    }
  }
  return result;
}

function validate(room: Room) {
  const items = [room.start, room.exit, room.key, room.door];
  if (new Set(items).size !== 4 || items.some((cell) => !inside(cell) || wall(room, cell))) {
    return 'Place robot, key, door, and exit on four different floor tiles.';
  }
  if (distances(room, room.start, true)[room.key] === 999) {
    return 'The key is unreachable while the door is locked.';
  }
  if (distances(room, room.key)[room.exit] === 999) {
    return 'There is no path from the key to the exit.';
  }
  return undefined;
}

function freshRoom(): Room {
  const pick = <T,>(items: T[]) => items[Math.floor(Math.random() * items.length)];
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const floors = new Set([9 + Math.floor(Math.random() * 6) + 8 * Math.floor(Math.random() * 6)]);
    while (true) {
      const choices = Array.from({ length: 64 }, (_, cell) => cell).filter(
        (cell) => inside(cell) && !floors.has(cell) && neighbors(cell).filter((next) => floors.has(next)).length === 1
      );
      if (!choices.length) break;
      floors.add(pick(choices));
    }
    if (floors.size < 15) continue;

    let bits = (1n << 64n) - 1n;
    for (const cell of floors) bits &= ~(1n << BigInt(cell));
    const cells = [...floors];
    const start = pick(cells);
    const room: Room = {
      walls: bits.toString(16).padStart(16, '0'),
      start,
      heading: Math.floor(Math.random() * 4),
      exit: 0,
      key: 0,
      door: 0
    };
    const fromStart = distances(room, start);
    room.exit = cells.reduce((furthest, cell) => (fromStart[furthest] > fromStart[cell] ? furthest : cell));
    const path = [room.exit];
    let at = room.exit;
    while (at !== start) {
      at = neighbors(at).find((cell) => fromStart[cell] + 1 === fromStart[at])!;
      path.push(at);
    }
    if (path.length < 5) continue;
    room.door = path[Math.floor(path.length / 2)];
    const reachable = distances(room, start, true);
    const keys = cells.filter((cell) => reachable[cell] !== 999 && ![start, room.exit, room.door].includes(cell));
    if (!keys.length) continue;
    room.key = pick(keys);
    return room;
  }
  throw new Error('Could not generate a room.');
}

const copyRoom = (room: Room): Room => ({ ...room });

function sourceForDisplay(source: unknown) {
  if (typeof source !== 'string') return undefined;
  let depth = 0;
  return source
    .replace(/([{};])/g, '$1\n')
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      if (line.startsWith('}')) depth -= 1;
      const formatted = `${'  '.repeat(Math.max(0, depth))}${line}`;
      if (line.endsWith('{')) depth += 1;
      return formatted;
    })
    .join('\n');
}

function drawIcon(
  context: CanvasRenderingContext2D,
  kind: 'key' | 'door' | 'exit' | 'robot',
  x: number,
  y: number,
  size: number,
  heading = 0
) {
  context.save();
  context.translate(x, y);
  context.lineWidth = size * 0.08;
  context.lineCap = 'round';
  context.lineJoin = 'round';

  if (kind === 'key') {
    context.strokeStyle = '#f5f5f6';
    context.beginPath();
    context.arc(-size * 0.15, 0, size * 0.16, 0, Math.PI * 2);
    context.moveTo(size * 0.01, 0);
    context.lineTo(size * 0.32, 0);
    context.lineTo(size * 0.32, size * 0.13);
    context.moveTo(size * 0.2, 0);
    context.lineTo(size * 0.2, size * 0.09);
    context.stroke();
  }

  if (kind === 'door') {
    context.fillStyle = '#7b7d83';
    context.fillRect(-size * 0.25, -size * 0.33, size * 0.5, size * 0.66);
    context.strokeStyle = '#dedee0';
    context.strokeRect(-size * 0.25, -size * 0.33, size * 0.5, size * 0.66);
    context.fillStyle = '#08090b';
    context.fillRect(size * 0.07, -size * 0.02, size * 0.07, size * 0.07);
  }

  if (kind === 'exit') {
    context.strokeStyle = '#f5f5f6';
    context.beginPath();
    context.arc(0, 0, size * 0.28, 0, Math.PI * 2);
    context.moveTo(-size * 0.14, 0);
    context.lineTo(size * 0.14, 0);
    context.moveTo(size * 0.14, 0);
    context.lineTo(size * 0.03, -size * 0.1);
    context.moveTo(size * 0.14, 0);
    context.lineTo(size * 0.03, size * 0.1);
    context.stroke();
  }

  if (kind === 'robot') {
    context.rotate((heading * Math.PI) / 2);
    context.fillStyle = '#aeb0b5';
    context.beginPath();
    context.moveTo(0, -size * 0.29);
    context.lineTo(size * 0.24, size * 0.22);
    context.lineTo(-size * 0.24, size * 0.22);
    context.closePath();
    context.fill();
    context.fillStyle = '#08090b';
    context.fillRect(-size * 0.08, -size * 0.05, size * 0.16, size * 0.13);
  }
  context.restore();
}

function drawBoard(
  canvas: HTMLCanvasElement | null,
  room: Room,
  frames: Frame[] | undefined,
  frame: number,
  mini = false
) {
  if (!canvas) return;
  const size = mini ? 112 : 560;
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) return;

  const cellSize = size / 8;
  const state = frames?.[Math.min(frame, Math.max(0, frames.length - 1))] ?? [room.start, room.heading, 0, false];
  const visited = new Set((frames ?? []).slice(0, frame + 1).map(([cell]) => cell));
  context.fillStyle = '#08090b';
  context.fillRect(0, 0, size, size);

  for (let cell = 0; cell < 64; cell += 1) {
    const x = (cell % 8) * cellSize;
    const y = Math.floor(cell / 8) * cellSize;
    context.fillStyle = wall(room, cell) ? '#27292e' : visited.has(cell) ? '#b3b4b8' : '#dedee0';
    context.fillRect(x + 1, y + 1, cellSize - 2, cellSize - 2);
    if (!mini && wall(room, cell)) {
      context.fillStyle = '#34363b';
      context.fillRect(x + cellSize * 0.14, y + cellSize * 0.14, cellSize * 0.72, 2);
    }
  }

  if (!mini && frames?.length) {
    context.strokeStyle = 'rgb(8 9 11 / 45%)';
    context.lineWidth = 3;
    context.beginPath();
    frames.slice(0, frame + 1).forEach(([cell], index) => {
      const x = (cell % 8 + 0.5) * cellSize;
      const y = (Math.floor(cell / 8) + 0.5) * cellSize;
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    });
    context.stroke();
  }

  const item = (kind: 'key' | 'door' | 'exit' | 'robot', cell: number, direction?: number) =>
    drawIcon(context, kind, (cell % 8 + 0.5) * cellSize, (Math.floor(cell / 8) + 0.5) * cellSize, cellSize * 0.78, direction);

  item('exit', room.exit);
  if (!state[3]) {
    item('key', room.key);
    item('door', room.door);
  } else {
    context.strokeStyle = '#7b7d83';
    context.lineWidth = 2;
    context.strokeRect(
      (room.door % 8) * cellSize + cellSize * 0.22,
      Math.floor(room.door / 8) * cellSize + cellSize * 0.12,
      cellSize * 0.56,
      cellSize * 0.76
    );
  }

  if (!mini && frames?.length) {
    const observed = sensors(room, state[0], state[1], state[3]);
    [state[1], (state[1] + 3) & 3, (state[1] + 1) & 3].forEach((direction, index) => {
      const cell = neighbor(state[0], direction);
      context.strokeStyle = observed[index] ? '#5e6066' : '#7b7d83';
      context.lineWidth = 3;
      context.strokeRect(
        (cell % 8) * cellSize + 5,
        Math.floor(cell / 8) * cellSize + 5,
        cellSize - 10,
        cellSize - 10
      );
    });
  }
  item('robot', state[0], state[1]);
}

function PreviewCanvas({ preview }: { preview: Preview }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    drawBoard(canvas.current, preview.room, preview.frames, Math.max(0, preview.frames.length - 1), true);
  }, [preview]);
  return <canvas ref={canvas} aria-hidden="true" />;
}

function isRoom(value: unknown): value is Room {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Room;
  return (
    typeof candidate.walls === 'string' &&
    [candidate.start, candidate.heading, candidate.key, candidate.door, candidate.exit].every(
      (item) => typeof item === 'number'
    )
  );
}

function parseFrames(value: unknown): Frame[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((frame) => {
    if (
      !Array.isArray(frame) ||
      frame.length < 4 ||
      typeof frame[0] !== 'number' ||
      typeof frame[1] !== 'number' ||
      typeof frame[2] !== 'number' ||
      typeof frame[3] !== 'boolean'
    ) {
      return [];
    }
    return [[frame[0], frame[1], frame[2], frame[3]] as Frame];
  });
}

function parsePreviews(value: unknown): Preview[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((value) => {
    if (!value || typeof value !== 'object' || !isRoom((value as Preview).room)) return [];
    const preview = value as Preview;
    return [{ room: copyRoom(preview.room), frames: parseFrames(preview.frames), solved: Boolean(preview.solved) }];
  });
}

function hasBrain(event: TinyRobotEvent | undefined) {
  return Array.isArray(event?.decisions) && event.decisions.length === 16;
}

function latestBrain(events: TinyRobotEvent[], current?: TinyRobotEvent) {
  if (hasBrain(current)) return current;
  for (let index = events.length - 1; index >= 0; index -= 1) {
    if (hasBrain(events[index])) return events[index];
  }
  return undefined;
}

function reading(value: boolean | undefined) {
  return value === undefined ? '—' : value ? 'OPEN' : 'WALL';
}

/**
 * Full Tiny Robot room editor and result inspector. The runner that owns replay
 * and GPU execution stays outside this component; pass its sample or live events
 * here to show the same brain in the editor.
 */
export default function TinyRobotParity({ recording, events, current }: TinyRobotParityProps) {
  const recordingEvents = Array.isArray(recording)
    ? recording
    : Array.isArray(recording?.events)
      ? recording.events
      : [];
  const eventList = events ?? recordingEvents;
  // During a replay the runner keeps the whole recording in memory. Limit the
  // search to the visible event so the rule table evolves with the replay,
  // rather than jumping straight to the final recorded brain.
  const currentIndex = current ? eventList.indexOf(current) : -1;
  const visibleEvents = currentIndex >= 0 ? eventList.slice(0, currentIndex + 1) : eventList;
  const brain = latestBrain(visibleEvents, current);
  const decisions = Array.isArray(brain?.decisions) ? (brain.decisions as number[]) : [];
  const brainSignature = brain?.brain ?? decisions.join(',');
  const previews = useMemo(() => parsePreviews(brain?.previews), [brain?.previews]);
  const galleryKind = brain?.kind === 'done' ? 'Unseen test rooms' : 'Sample training rooms';
  const [room, setRoom] = useState<Room>(() => freshRoom());
  const roomRef = useRef(room);
  const [roomKind, setRoomKind] = useState('Fresh maze');
  const [tool, setTool] = useState<Tool>('wall');
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(12);
  const [selectedPreview, setSelectedPreview] = useState<number>();
  const drag = useRef({ active: false, wallValue: true, lastCell: -1 });
  const canvas = useRef<HTMLCanvasElement>(null);

  const validity = useMemo(() => validate(room), [room]);
  const trace = useMemo(
    () => (decisions.length === 16 && !validity ? simulate(decisions, room) : undefined),
    [brainSignature, decisions, room, validity]
  );
  const maxFrame = Math.max(0, (trace?.frames.length ?? 1) - 1);
  const visibleFrame = Math.min(frame, maxFrame);
  const state = trace?.frames[visibleFrame];
  const [front, left, right] = state ? sensors(room, state[0], state[1], state[3]) : [undefined, undefined, undefined];
  const row = state ? Number(front) + 2 * Number(left) + 4 * Number(right) + 8 * state[2] : undefined;
  const instruction = row === undefined ? undefined : decisions[row] ?? 0;
  const finished = Boolean(trace && visibleFrame === maxFrame);

  useEffect(() => {
    drawBoard(canvas.current, room, trace?.frames, visibleFrame);
  }, [room, trace?.frames, visibleFrame]);

  useEffect(() => {
    setPlaying(false);
    setFrame(0);
  }, [brainSignature]);

  useEffect(() => {
    if (!playing || !trace) return;
    const timer = window.setInterval(() => {
      setFrame((previous) => {
        if (previous >= trace.frames.length - 1) {
          setPlaying(false);
          return previous;
        }
        return previous + 1;
      });
    }, 1000 / speed);
    return () => window.clearInterval(timer);
  }, [playing, speed, trace]);

  const commitRoom = (next: Room, nextKind: string, previewIndex?: number) => {
    roomRef.current = next;
    setRoom(next);
    setRoomKind(nextKind);
    setSelectedPreview(previewIndex);
    setPlaying(false);
    setFrame(0);
  };

  const fresh = () => commitRoom(freshRoom(), 'Fresh maze');

  const rotate = () => {
    const previous = roomRef.current;
    commitRoom({ ...previous, heading: (previous.heading + 1) & 3 }, 'Your room');
  };

  const paint = (cell: number, first: boolean) => {
    if (!inside(cell)) return;
    const previous = roomRef.current;
    const occupied: Array<keyof Pick<Room, 'start' | 'key' | 'door' | 'exit'>> = ['start', 'key', 'door', 'exit'];
    let next: Room | undefined;

    if (tool === 'wall') {
      if (occupied.some((item) => previous[item] === cell)) return;
      if (first) drag.current.wallValue = !wall(previous, cell);
      const bits = BigInt(`0x${previous.walls}`);
      const mask = 1n << BigInt(cell);
      const nextBits = drag.current.wallValue ? bits | mask : bits & ~mask;
      next = { ...previous, walls: nextBits.toString(16).padStart(16, '0') };
    } else {
      if (occupied.some((item) => item !== tool && previous[item] === cell)) return;
      const bits = BigInt(`0x${previous.walls}`) & ~(1n << BigInt(cell));
      next = { ...previous, [tool]: cell, walls: bits.toString(16).padStart(16, '0') } as Room;
    }
    commitRoom(next, 'Your room');
  };

  const cellFromPointer = (event: JSX.TargetedPointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = Math.min(7, Math.max(0, Math.floor(((event.clientX - rect.left) / rect.width) * 8)));
    const y = Math.min(7, Math.max(0, Math.floor(((event.clientY - rect.top) / rect.height) * 8)));
    return y * 8 + x;
  };

  const onPointerDown = (event: JSX.TargetedPointerEvent<HTMLCanvasElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current.active = true;
    drag.current.lastCell = cellFromPointer(event);
    paint(drag.current.lastCell, true);
  };

  const onPointerMove = (event: JSX.TargetedPointerEvent<HTMLCanvasElement>) => {
    if (!drag.current.active || tool !== 'wall') return;
    const cell = cellFromPointer(event);
    if (cell === drag.current.lastCell) return;
    drag.current.lastCell = cell;
    paint(cell, false);
  };

  const endDrag = (event: JSX.TargetedPointerEvent<HTMLCanvasElement>) => {
    drag.current.active = false;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  const togglePlayback = () => {
    if (!trace) return;
    if (visibleFrame >= maxFrame) setFrame(0);
    setPlaying((value) => !value);
  };

  const step = () => {
    if (!trace) return;
    setPlaying(false);
    setFrame((previous) => Math.min(previous + 1, maxFrame));
  };

  const reset = () => {
    setPlaying(false);
    setFrame(0);
  };

  const outcome = validity
    ? validity
    : !brain
      ? 'Waiting for a brain'
      : finished
        ? trace?.solved
          ? `Escaped in ${trace.steps} actions!`
          : 'Not solved within 256 actions'
        : playing
          ? 'Exploring…'
          : visibleFrame
            ? 'Paused'
            : 'Ready to explore';

  const decisionText = !state
    ? brain
      ? 'Fix the room to continue'
      : 'Waiting for a brain'
    : finished
      ? trace?.solved
        ? 'Exit reached'
        : 'Action limit reached'
      : actions[(instruction ?? 0) & 3];

  return (
    <div class="tiny-robot-parity">
      <div class="experiment-robot-layout">
        <section class="tiny-robot-panel" aria-label="Tiny Robot room editor">
          <div class="tiny-robot-panel-heading">
            <div>
              <h3>Your test room</h3>
              <p>{roomKind}</p>
            </div>
            <button type="button" class="experiment-button" onClick={fresh}>Fresh maze</button>
          </div>
          <canvas
            ref={canvas}
            class="experiment-robot-canvas tiny-robot-canvas"
            aria-label="Editable eight by eight robot maze"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
          />
          <div class="tiny-robot-tools" aria-label="Room drawing tools">
            <span>Draw</span>
            {(
              [
                ['wall', 'Wall / floor'],
                ['start', 'Robot'],
                ['key', 'Key'],
                ['door', 'Door'],
                ['exit', 'Exit']
              ] as Array<[Tool, string]>
            ).map(([value, label]) => (
              <button
                type="button"
                class={`experiment-button${tool === value ? ' tiny-robot-tool-active' : ''}`}
                aria-pressed={tool === value}
                onClick={() => setTool(value)}
              >
                {label}
              </button>
            ))}
            <button type="button" class="experiment-button" onClick={rotate}>Rotate robot</button>
          </div>
          <div class="experiment-robot-controls tiny-robot-playback">
            <button type="button" class="experiment-button experiment-button-primary" onClick={togglePlayback} disabled={!trace}>
              {playing ? 'Pause' : 'Run this brain'}
            </button>
            <button type="button" class="experiment-button" onClick={step} disabled={!trace}>Step</button>
            <button type="button" class="experiment-button" onClick={reset} disabled={!trace}>Reset</button>
            <label>
              Speed
              <select value={speed} onInput={(event) => setSpeed(Number((event.currentTarget as HTMLSelectElement).value))}>
                <option value={6}>6 actions/s</option>
                <option value={12}>12 actions/s</option>
                <option value={30}>30 actions/s</option>
              </select>
            </label>
          </div>
          <div class={`tiny-robot-outcome${validity ? ' tiny-robot-outcome-invalid' : ''}`} aria-live="polite">
            <strong>{outcome}</strong>
            <span>{visibleFrame} / 256 actions</span>
          </div>
          <p class="tiny-robot-help">Click or drag to draw walls. Move the key, door, and exit to challenge this brain.</p>
        </section>

        <section class="tiny-robot-panel" aria-label="Tiny Robot decision table">
          <div class="tiny-robot-panel-heading">
            <div>
              <h3>Inside the robot</h3>
              <p>{brain ? 'Discovered brain' : 'No brain loaded'}</p>
            </div>
          </div>
          <div class="tiny-robot-sensors">
            <div><span>Left</span><strong>{reading(left)}</strong></div>
            <div><span>Ahead</span><strong>{reading(front)}</strong></div>
            <div><span>Right</span><strong>{reading(right)}</strong></div>
            <div><span>Memory</span><strong>{state?.[2] ?? '—'}</strong></div>
          </div>
          <div class="tiny-robot-decision">
            <span>Next instruction</span>
            <strong>{decisionText}</strong>
            <small>
              {!state || finished
                ? finished
                  ? 'Episode complete'
                  : ''
                : `memory ${state[2]} → ${(instruction ?? 0) >> 2} · ${state[3] ? 'key collected' : 'key not collected'}`}
            </small>
          </div>
          <div class="tiny-robot-rules-wrap">
            <table class="tiny-robot-rules">
              <thead>
                <tr><th>Mem</th><th>Ahead</th><th>Left</th><th>Right</th><th>Action</th><th>Next mem</th></tr>
              </thead>
              <tbody>
                {Array.from({ length: 16 }, (_, rule) => {
                  const decision = decisions[rule] ?? 0;
                  return (
                    <tr class={row === rule && !finished ? 'tiny-robot-rule-active' : undefined}>
                      <td>{rule >> 3}</td>
                      <td>{rule & 1 ? 'open' : 'wall'}</td>
                      <td>{rule & 2 ? 'open' : 'wall'}</td>
                      <td>{rule & 4 ? 'open' : 'wall'}</td>
                      <td>{brain ? actions[decision & 3] : '—'}</td>
                      <td>{brain ? decision >> 2 : '—'}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {sourceForDisplay(brain?.brain_source) && (
            <details class="tiny-robot-source">
              <summary>Read the discovered Gremlin program</summary>
              <pre class="experiment-code">{sourceForDisplay(brain?.brain_source)}</pre>
            </details>
          )}
        </section>
      </div>

      {previews.length > 0 && (
        <section class="tiny-robot-gallery-section" aria-label={galleryKind}>
          <div class="tiny-robot-panel-heading">
            <div>
              <h3>{galleryKind}</h3>
              <p>Choose a recorded room to inspect it.</p>
            </div>
          </div>
          <div class="tiny-robot-gallery">
            {previews.map((preview, index) => (
              <button
                type="button"
                class={`tiny-robot-gallery-item${selectedPreview === index ? ' tiny-robot-gallery-selected' : ''}`}
                aria-pressed={selectedPreview === index}
                onClick={() => commitRoom(copyRoom(preview.room), galleryKind.slice(0, -1), index)}
              >
                <PreviewCanvas preview={preview} />
                <span>{preview.solved ? 'Escaped' : 'Unsolved'}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      <div class="experiment-metrics tiny-robot-metrics">
        <div class="experiment-metric">
          <span>training rooms</span>
          <strong>{typeof brain?.solved === 'number' ? `${brain.solved} / ${brain.cases}` : '—'}</strong>
        </div>
        <div class="experiment-metric">
          <span>unseen rooms</span>
          <strong>{typeof brain?.holdout_solved === 'number' ? `${brain.holdout_solved} / ${brain.holdout_cases}` : '—'}</strong>
        </div>
        <div class="experiment-metric">
          <span>generation</span>
          <strong>{brain?.generation ?? '—'}</strong>
        </div>
        <div class="experiment-metric">
          <span>simulated episodes</span>
          <strong>{formatNumber(brain?.evaluations)}</strong>
        </div>
      </div>
    </div>
  );
}
