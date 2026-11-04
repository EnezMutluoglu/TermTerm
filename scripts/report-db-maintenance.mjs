import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
const report=JSON.parse(await fs.readFile('artifacts/database-workspace/change-plan-tests.json','utf8'));
assert.equal(report.report.length,3);assert.ok(report.report.every(r=>r.status==='passed'));
const ids={postgresql:'PG-OPS-004',mssql:'MS-OPS-003',oracle:'OR-OPS-003'};
await fs.writeFile('src/database/maintenanceValidation.json',JSON.stringify({date:report.date,actions:Object.fromEntries(report.report.map(r=>[ids[r.engine],{engine:r.engine,version:r.version,status:'maintenance-passed',tests:r.tests}]))},null,2)+'\n');
