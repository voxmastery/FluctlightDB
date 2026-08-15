import type {CSSProperties, ReactNode} from "react";
import {AbsoluteFill, Easing, interpolate, spring, useCurrentFrame, useVideoConfig} from "remotion";

import {openingTranscript, rootSplitTranscript, verificationTranscript, workerATranscript, workerBTranscript, type StyledTerminalEntry, type Tone} from "./transcript";
import {visibleTerminalEntries} from "./timeline";

const palette = {
  ink: "#080A10", terminal: "#0C1019", panel: "#101622", line: "#242D3D",
  text: "#D9E2EE", muted: "#718096", cyan: "#5DE7D7", amber: "#F5BA62",
  red: "#FF7185", violet: "#9E8CFF",
};
const mono = '"DejaVu Sans Mono", "Liberation Mono", monospace';
const clamp = {extrapolateLeft: "clamp" as const, extrapolateRight: "clamp" as const};
const toneColor: Record<Tone, string> = {
  muted: palette.muted, normal: palette.text, cyan: palette.cyan,
  amber: palette.amber, red: palette.red, violet: palette.violet,
};

const StatusDot = ({color}: {color: string}) => <span style={{display: "inline-block", width: 12, height: 12, borderRadius: 99, background: color, boxShadow: `0 0 18px ${color}88`}} />;

const TerminalLines = ({entries, frame, fontSize = 24, lineHeight = 38}: {entries: readonly StyledTerminalEntry[]; frame: number; fontSize?: number; lineHeight?: number}) => {
  const visible = visibleTerminalEntries(entries, frame);
  return <div style={{fontFamily: mono, fontSize, lineHeight: `${lineHeight}px`}}>
    {visible.map((entry, index) => {
      const source = entries[index];
      const color = toneColor[source.tone ?? "normal"];
      const cursor = source.kind === "command" && !entry.complete;
      return <div key={`${source.frame}-${source.text}`} style={{minHeight: lineHeight, color, paddingLeft: (source.indent ?? 0) * 24, whiteSpace: "pre", textShadow: source.tone === "cyan" ? `0 0 20px ${palette.cyan}22` : undefined}}>
        {source.kind === "command" ? <span style={{color: palette.cyan}}>$ </span> : null}
        {source.kind === "prompt" ? <span style={{color: palette.violet}}>› </span> : null}
        {entry.text}
        {cursor ? <span style={{display: "inline-block", width: 11, height: fontSize, marginLeft: 3, background: palette.cyan, verticalAlign: -4}} /> : null}
      </div>;
    })}
  </div>;
};

const Pane = ({children, style, active = false}: {children: ReactNode; style?: CSSProperties; active?: boolean}) => <div style={{position: "relative", overflow: "hidden", background: palette.panel, border: `1px solid ${active ? palette.cyan + "88" : palette.line}`, borderRadius: 10, boxShadow: active ? `inset 0 0 50px ${palette.cyan}08, 0 0 32px ${palette.cyan}0A` : "none", ...style}}>{children}</div>;

const CaptionRail = ({frame}: {frame: number}) => {
  const captions = [
    {from: 0, text: "Codex workflow visualization — backed by runnable demo output"},
    {from: 360, text: "Shared truth. Different strategies. Zero duplicated memory IDs."},
    {from: 665, text: "Workers report outcomes. Only trusted evidence changes memory."},
    {from: 900, text: "Now run the public behavioral proof end to end."},
    {from: 1280, text: "Coordinator stopped. State recovered from WAL after restart."},
    {from: 1515, text: "Parallel Codex agents that remember together—without thinking the same way."},
  ];
  const currentIndex = Math.max(0, captions.findLastIndex((caption) => frame >= caption.from));
  const current = captions[currentIndex];
  const age = frame - current.from;
  const enter = interpolate(age, [0, 18], [18, 0], {...clamp, easing: Easing.out(Easing.cubic)});
  const opacity = interpolate(age, [0, 12], [0, 1], clamp);
  return <div style={{position: "absolute", left: 0, right: 0, bottom: -58, display: "flex", justifyContent: "center", transform: `translateY(${enter}px)`, opacity}}>
    <div style={{fontFamily: mono, color: "#A8B6C8", fontSize: 19, letterSpacing: 0.2, background: "#0D121CCC", border: `1px solid ${palette.line}`, borderRadius: 99, padding: "10px 22px"}}>{current.text}</div>
  </div>;
};

const SplitWorkspace = ({frame}: {frame: number}) => {
  const localFrame = frame - 388;
  const enter = spring({frame: localFrame, fps: 30, config: {damping: 19, stiffness: 120, mass: 0.9}});
  const exit = interpolate(frame, [845, 900], [1, 0], {...clamp, easing: Easing.in(Easing.cubic)});
  const progress = enter * exit;
  return <div style={{position: "absolute", inset: 20, display: "grid", gridTemplateColumns: "0.72fr 1.28fr", gridTemplateRows: "1fr 1fr", gap: 12, opacity: progress, transform: `scale(${interpolate(progress, [0, 1], [0.985, 1])}) translateY(${interpolate(progress, [0, 1], [20, 0])}px)`}}>
    <Pane style={{gridRow: "1 / 3", padding: "28px 30px"}} active={frame > 690}>
      <TerminalLines entries={rootSplitTranscript} frame={localFrame} fontSize={21} lineHeight={35} />
      <div style={{position: "absolute", left: 30, right: 30, bottom: 30}}>
        <div style={{fontFamily: mono, fontSize: 14, color: palette.muted, marginBottom: 9}}>DURABLE TRANSACTION LOG</div>
        <div style={{height: 7, borderRadius: 99, background: "#202A39", overflow: "hidden"}}><div style={{height: "100%", width: `${interpolate(localFrame, [0, 440], [12, 100], clamp)}%`, borderRadius: 99, background: `linear-gradient(90deg, ${palette.violet}, ${palette.cyan})`}} /></div>
      </div>
    </Pane>
    <Pane style={{padding: "22px 28px"}} active={frame >= 620 && frame < 760}><TerminalLines entries={workerATranscript} frame={localFrame} fontSize={20} lineHeight={31} /></Pane>
    <Pane style={{padding: "22px 28px"}} active={frame >= 640 && frame < 800}><TerminalLines entries={workerBTranscript} frame={localFrame} fontSize={20} lineHeight={31} /></Pane>
  </div>;
};

export const SwarmTerminalDemo = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const boot = spring({frame, fps, config: {damping: 18, stiffness: 90, mass: 1.1}});
  const splitVisible = interpolate(frame, [372, 410, 850, 905], [0, 1, 1, 0], clamp);
  const mainVisible = 1 - splitVisible;
  const ambientX = Math.sin(frame / 110) * 28;
  const recPulse = 0.55 + Math.sin(frame / 11) * 0.2;
  return <AbsoluteFill style={{background: palette.ink, color: palette.text, fontFamily: mono, overflow: "hidden"}}>
    <div style={{position: "absolute", width: 800, height: 800, left: 1030 + ambientX, top: -390, borderRadius: "50%", background: `radial-gradient(circle, ${palette.violet}24 0%, ${palette.violet}08 46%, transparent 72%)`, filter: "blur(12px)"}} />
    <div style={{position: "absolute", width: 900, height: 700, left: -340 - ambientX * 0.5, top: 610, borderRadius: "50%", background: `radial-gradient(circle, ${palette.cyan}16 0%, transparent 70%)`, filter: "blur(20px)"}} />
    <div style={{position: "absolute", top: 38, left: 66, display: "flex", alignItems: "center", gap: 16}}><div style={{fontSize: 18, letterSpacing: 1.7, color: palette.muted}}>FLUCTLIGHT SWARM MEMORY</div><div style={{height: 18, width: 1, background: palette.line}} /><div style={{fontSize: 16, color: palette.cyan}}>CODEX WORKFLOW DEMO</div></div>
    <div style={{position: "absolute", top: 36, right: 66, display: "flex", gap: 11, alignItems: "center", fontSize: 15, color: palette.muted}}><span style={{opacity: recPulse}}>●</span>SCRIPTED WORKFLOW · REAL DEMO OUTPUT</div>
    <div style={{position: "absolute", left: 66, right: 66, top: 92, height: 880, borderRadius: 18, overflow: "visible", opacity: boot, transform: `translateY(${interpolate(boot, [0, 1], [54, 0])}px) scale(${interpolate(boot, [0, 1], [0.94, 1])})`, transformOrigin: "center", boxShadow: "0 44px 120px #000A, 0 0 0 1px #283246"}}>
      <div style={{height: 54, borderRadius: "18px 18px 0 0", background: "#171D29", borderBottom: `1px solid ${palette.line}`, display: "flex", alignItems: "center", padding: "0 22px", gap: 12}}>
        <StatusDot color="#FF6B6B" /><StatusDot color="#F7C65A" /><StatusDot color="#61D577" />
        <div style={{position: "absolute", left: 0, right: 0, textAlign: "center", fontSize: 16, color: "#8A99AC"}}>codex — FluctlightDB — main</div>
        <div style={{marginLeft: "auto", display: "flex", alignItems: "center", gap: 9, fontSize: 14, color: palette.cyan}}><StatusDot color={palette.cyan} /> WAL SYNCED</div>
      </div>
      <div style={{position: "relative", height: 826, borderRadius: "0 0 18px 18px", background: palette.terminal, overflow: "hidden"}}>
        <div style={{position: "absolute", inset: "34px 42px", opacity: mainVisible, transform: `translateY(${interpolate(splitVisible, [0, 1], [0, -16])}px) scale(${interpolate(splitVisible, [0, 1], [1, 0.985])})`}}>
          {frame < 900 ? <TerminalLines entries={openingTranscript} frame={frame} fontSize={24} lineHeight={42} /> : <TerminalLines entries={verificationTranscript} frame={frame} fontSize={23} lineHeight={39} />}
        </div>
        <SplitWorkspace frame={frame} />
        <div style={{pointerEvents: "none", position: "absolute", inset: 0, opacity: 0.12, backgroundImage: "repeating-linear-gradient(0deg, transparent 0px, transparent 3px, #FFFFFF 4px)", mixBlendMode: "overlay"}} />
      </div>
      <CaptionRail frame={frame} />
    </div>
  </AbsoluteFill>;
};
