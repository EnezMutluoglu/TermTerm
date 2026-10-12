import {config as base} from './wdio.team-v2.conf.mjs';
export const config={...base,specs:['./tests/desktop/team-offline.mjs']};
