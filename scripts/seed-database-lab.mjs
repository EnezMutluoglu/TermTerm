import {readFile} from 'node:fs/promises';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper();
const profiles=JSON.parse(await readFile('.lab/database/profiles.json','utf8'));
try{for(const p of profiles){
 if(!['localhost','127.0.0.1'].includes(p.host)||![41432,41433,41521].includes(p.port))throw Error('Only disposable local lab endpoints are allowed');
 let sessionId;try{({sessionId}=await rpc({op:'open',profile:p}));}catch{continue;}
 const query=(sql,parameters={})=>rpc({op:'query',sessionId,sql,parameters});
 try{
  const type=p.engine==='oracle'?'NVARCHAR2(100)':p.engine==='mssql'?'NVARCHAR(100)':'VARCHAR(100)';
  try{await query(`CREATE TABLE tt_demo_orders (order_id INT PRIMARY KEY, customer_name ${type}, region ${type}, total_amount DECIMAL(12,2), status ${type})`);}catch(e){if(!['42P07','S0001'].includes(e.sqlState)&&![955,2714].includes(e.vendorCode))throw e;}
  if((await query('SELECT COUNT(*) FROM tt_demo_orders')).results[0].rows[0][0]==='0'){
   const rows=[[1001,'Atlas Teknoloji','İstanbul',12500.75,'Tamamlandı'],[1002,'Mavi Yazılım','Ankara',8450.00,'Hazırlanıyor'],[1003,'Ege Sistem','İzmir',21900.50,'Tamamlandı'],[1004,'Kuzey Bilişim','Samsun',4800.25,'Bekliyor'],[1005,'Güney Veri','Antalya',16320.00,'Tamamlandı']];
   for(const [id,name,region,amount,status]of rows)await query('INSERT INTO tt_demo_orders VALUES (:id,:name,:region,:amount,:status)',{id,name,region,amount,status});
  }
  console.log(p.engine+': disposable demonstration rows ready');
 }finally{await rpc({op:'close',sessionId});}
}}finally{close();}
