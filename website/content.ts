import { home } from "./home";
import { app } from "./docs/app";
import { cli } from "./docs/cli";
import { config } from "./docs/config";
import { data } from "./docs/data";
import { install } from "./docs/install";
import { overview } from "./docs/index";
import { mcp } from "./docs/mcp";
import { memory } from "./docs/memory";
import { mesh } from "./docs/mesh";
import { security } from "./docs/security";
import { skills } from "./docs/skills";
import { troubleshoot } from "./docs/troubleshoot";
import { vault } from "./docs/vault";
import { connect } from "./tutorials/connect";
import { continuity } from "./tutorials/continuity";
import { curate } from "./tutorials/curate";
import { tutorials } from "./tutorials/index";
import { launch } from "./tutorials/launch";
import { learn } from "./tutorials/learn";
import { lifecycle } from "./tutorials/lifecycle";
import { meshtutorial } from "./tutorials/mesh";
import { overseer } from "./tutorials/overseer";
import { recovery } from "./tutorials/recovery";
import { secrets } from "./tutorials/secrets";
import { skills as skillstutorial } from "./tutorials/skills";
import type { Page } from "./types";

export const pages: Page[] = [
  home,
  overview,
  install,
  app,
  memory,
  mcp,
  mesh,
  skills,
  vault,
  data,
  config,
  security,
  cli,
  troubleshoot,
  tutorials,
  connect,
  continuity,
  curate,
  secrets,
  skillstutorial,
  learn,
  launch,
  meshtutorial,
  overseer,
  recovery,
  lifecycle,
];
