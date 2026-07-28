import { home } from "./home";
import { app } from "./docs/app";
import { cli } from "./docs/cli";
import { config } from "./docs/config";
import { data } from "./docs/data";
import { install } from "./docs/install";
import { overview } from "./docs/index";
import { mcp } from "./docs/mcp";
import { memory } from "./docs/memory";
import { security } from "./docs/security";
import { troubleshoot } from "./docs/troubleshoot";
import { vault } from "./docs/vault";
import { connect } from "./tutorials/connect";
import { continuity } from "./tutorials/continuity";
import { curate } from "./tutorials/curate";
import { tutorials } from "./tutorials/index";
import { recovery } from "./tutorials/recovery";
import { secrets } from "./tutorials/secrets";
import type { Page } from "./types";

export const pages: Page[] = [
  home,
  overview,
  install,
  app,
  memory,
  mcp,
  vault,
  data,
  config,
  security,
  cli,
  troubleshoot,
  tutorials,
  connect,
  continuity,
  secrets,
  curate,
  recovery,
];
