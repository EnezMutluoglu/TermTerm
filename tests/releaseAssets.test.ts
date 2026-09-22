import {it, expect} from 'vitest';
// @ts-expect-error Plain Node release script, intentionally independent of frontend build.
import {releaseAssets} from '../scripts/release-assets.mjs';
it('requires signed updater assets for all four targets and download packages for Linux/macOS',()=>{
  const plan=releaseAssets('0.3.3');
  expect(Object.keys(plan.platforms)).toEqual(['windows-x86_64','linux-x86_64','darwin-x86_64','darwin-aarch64']);
  for(const name of Object.values(plan.platforms)) {
    expect(plan.required).toContain(name);
    expect(plan.required).toContain(`${name}.sig`);
  }
  expect(plan.required).toContain('TermTerm_0.3.3_amd64.deb');
  expect(plan.required).toContain('TermTerm-0.3.3-1.x86_64.rpm');
  expect(new Set(plan.required).size).toBe(plan.required.length);
  expect(()=>releaseAssets('0.3.3-dev.1')).toThrow();
});
