import fs from 'node:fs/promises';
const c=JSON.parse(await fs.readFile('src/database/catalog.json','utf8'));const defs={};
const validation=JSON.parse(await fs.readFile('src/database/validation.json','utf8'));

for(const a of c.filter(a=>a.risk==='R0')){
 const params={};const sql=a.sql.join('\n');
 for(const m of sql.matchAll(/(?<!:):([a-zA-Z_]\w*)/g)){const name=m[1];let spec={type:'text',default:'',maxLength:256,nullable:/database_name|publication_name/.test(name)};
 if(/^(min_seconds|pid|object_id|session_id|row_limit|page_size|min_executions|child_number|lookback_hours|lookback_days|lookback_minutes)$/.test(name))spec={type:'integer',min:/^(min_executions|child_number)$/.test(name)?0:1,max:name==='lookback_hours'?48:name==='lookback_days'?(a.id==='OR-MON-032'?30:90):name==='lookback_minutes'?1440:/row_limit|page_size/.test(name)?(a.id==='OR-MON-013'?100:['OR-MON-010','OR-MON-025','OR-MON-027','OR-MON-032'].includes(a.id)?500:1000):2147483647,default:/row_limit|page_size/.test(name)?100:name==='min_seconds'?30:name==='lookback_days'?7:name==='lookback_hours'?24:name==='lookback_minutes'?60:1};
 if(name==='requested_roles')spec={type:'textArray',default:[],maxItems:100};
 if(/^(from_time|to_time)$/.test(name))spec={type:'datetime',default:'',maxLength:40};
 if(name==='sql_id')spec={...spec,pattern:'^[0-9a-z]{13}$'};
 if(/^(table_name|parent_table|qualified_table)$/.test(name))spec={...spec,type:a.engine==='oracle'?'tableName':'tableReference'};
 if(/^(schema_name|app_schema|owner_user)$/.test(name))spec={...spec,type:'schema'};
 if(name==='approved_session_name')spec={type:'enum',values:['system_health'],default:'system_health'};
 params[name]=spec;
 }
 for(const m of sql.matchAll(/\{\{(qualified|columns|ident|int):(\w+)\}\}/g)){const [,type,name]=m;params[name]=type==='int'?{type:'integer',min:1,max:2147483647,default:1}:type==='qualified'?{type:'object',default:{schema:'',table:''}}:type==='columns'?{type:'columns',default:[]}: {type:'column',default:''};}
 const resultUnits={};for(const name of validation.actions[a.id]?.resultColumns?.flat()??[]){const lower=name.toLowerCase();resultUnits[name]=/bytes|space_used|space_limit|space_reclaimable/.test(lower)?'bytes':/percent|pct/.test(lower)?'%':/_ms$|total_exec_time|mean_exec_time|max_exec_time/.test(lower)?'ms':/^(elapsed_time|cpu_time)$/.test(lower)&&a.engine==='oracle'?'µs':/seconds|_seconds|_s$/.test(lower)?'s':/_mb$/.test(lower)?'MiB':/blks|blocks/.test(lower)?'blocks':'value';}
 defs[a.id]={catalogVersion:2,resultUnits,parameters:params,minMajor:a.engine==='postgresql'?14:a.engine==='mssql'?15:19,maxMajor:a.engine==='postgresql'?18:a.engine==='mssql'?17:26,license:a.id==='OR-MON-045'||a.id==='OR-MON-046'?'disabled':null};
}
await fs.writeFile('src/database/actionInputs.json',JSON.stringify(defs,null,2)+'\n');console.log(Object.keys(defs).length+' R0 contracts generated');
