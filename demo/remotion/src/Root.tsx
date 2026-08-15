import {Composition} from "remotion";

import {SwarmTerminalDemo} from "./SwarmTerminalDemo";

export const RemotionRoot = () => (
  <Composition
    id="FluctlightSwarmTerminal"
    component={SwarmTerminalDemo}
    durationInFrames={1620}
    fps={30}
    width={1920}
    height={1080}
  />
);
