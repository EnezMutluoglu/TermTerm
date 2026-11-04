import com.google.gson.*;
import java.sql.*;
import java.util.*;

/** Metadata operations use a short-lived connection, never the editor transaction. */
public final class DbTools {
  static String q(String name,String engine){
    if(name==null||name.isBlank()||name.length()>128||name.chars().anyMatch(c->c<32))throw new IllegalArgumentException("Geçersiz nesne adı");
    return engine.equals("mssql")?"["+name.replace("]","]]")+"]":"\""+name.replace("\"","\"\"")+"\"";
  }
  static String pattern(DatabaseMetaData md,String value)throws SQLException{return value.replace(md.getSearchStringEscape(),md.getSearchStringEscape()+md.getSearchStringEscape()).replace("%",md.getSearchStringEscape()+"%").replace("_",md.getSearchStringEscape()+"_");}
  static List<Map<String,Object>> objects(ResultSet rs)throws SQLException{
    try(rs){List<Map<String,Object>> rows=new ArrayList<>();var md=rs.getMetaData();int size=0;while(rs.next()){if(rows.size()>=1000)throw new SQLException("Metadata 1000 satır sınırını aşıyor; sonuç eksik gösterilmedi", "54000");Map<String,Object> row=new LinkedHashMap<>();for(int i=1;i<=md.getColumnCount();i++){String value=rs.getString(i);if(value!=null){size+=value.length();if(value.length()>65536||size>2_000_000)throw new SQLException("Metadata boyut sınırını aşıyor; sonuç eksik gösterilmedi", "54000");}row.put(md.getColumnLabel(i).toLowerCase(java.util.Locale.ROOT),value);}rows.add(row);}return rows;}
  }
  static Map<String,Object> capabilities(Connection c,String engine)throws SQLException{
    var md=c.getMetaData();Map<String,Object> r=new LinkedHashMap<>();r.put("engine",engine);r.put("version",md.getDatabaseProductVersion());r.put("major",md.getDatabaseMajorVersion());r.put("database",c.getCatalog());r.put("schema",c.getSchema());r.put("username",md.getUserName());
    r.put("supported",engine.equals("postgresql")?md.getDatabaseMajorVersion()>=14&&md.getDatabaseMajorVersion()<=18:engine.equals("mssql")?md.getDatabaseMajorVersion()>=15&&md.getDatabaseMajorVersion()<=17:Set.of(19,21,23,26).contains(md.getDatabaseMajorVersion()));
    Map<String,String> probes=new LinkedHashMap<>();
    if(engine.equals("postgresql")){probes.put("identity","SELECT current_database() AS database_name,pg_postmaster_start_time()::text AS reset_epoch,pg_is_in_recovery() AS replica");probes.put("extensions","SELECT extname,extversion,nspname AS schema_name FROM pg_extension JOIN pg_namespace n ON n.oid=extnamespace");probes.put("permissions","SELECT rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user");}
    if(engine.equals("mssql")){probes.put("identity","SELECT DB_NAME() AS database_name,SERVERPROPERTY('Edition') AS edition,SERVERPROPERTY('EngineEdition') AS engine_edition,sqlserver_start_time AS reset_epoch FROM sys.dm_os_sys_info");probes.put("queryStore","SELECT actual_state_desc FROM sys.database_query_store_options");probes.put("permissions","SELECT permission_name FROM sys.fn_my_permissions(NULL,'DATABASE')");}
    if(engine.equals("oracle")){probes.put("identity","SELECT SYS_CONTEXT('USERENV','CON_NAME') AS database_name,SYS_CONTEXT('USERENV','CON_ID') AS con_id FROM dual");probes.put("compatible","SELECT value FROM v$parameter WHERE name='compatible'");probes.put("permissions","SELECT privilege FROM session_privs");probes.put("instance","SELECT startup_time AS reset_epoch,edition FROM v$instance");}
    for(var probe:probes.entrySet())try(Statement st=c.createStatement()){st.setQueryTimeout(5);st.setMaxRows(200);r.put(probe.getKey(),objects(st.executeQuery(probe.getValue())));}catch(SQLException e){r.put(probe.getKey(),Map.of("status",classify(e)));}
    if(engine.equals("postgresql")&&r.get("extensions") instanceof List<?> extensions)for(Object ext:extensions){var entry=(Map<String,Object>)ext;if("pg_stat_statements".equals(entry.get("extname")))try(Statement st=c.createStatement()){st.setQueryTimeout(5);r.put("queryStatsReset",objects(st.executeQuery("SELECT stats_reset::text,dealloc FROM "+q(entry.get("schema_name").toString(),engine)+".pg_stat_statements_info")));}catch(SQLException e){r.put("queryStatsReset",Map.of("status",classify(e)));}}
    return r;
  }
  static String classify(SQLException e){String s=Objects.toString(e.getSQLState(),"");int n=e.getErrorCode();if(s.startsWith("08")||Set.of("57P01","57P02","57P03").contains(s)||Set.of(28,3113,3114,596,233,17002,17008,17410).contains(n))return "unknown_outcome";if(s.equals("57014")||s.equals("HY008")||n==1013)return "cancelled";if(s.equals("42501")||Set.of(1031,229,297,300).contains(n))return "permission_denied";if(s.equals("42P01")||s.equals("42703")||Set.of(942,904,208,207).contains(n))return "unsupported";return "failed";}
  static Map<String,Object> detail(Connection c,String engine,String schema,String table)throws SQLException{
    var md=c.getMetaData();var columns=objects(md.getColumns(c.getCatalog(),pattern(md,schema),pattern(md,table),"%"));
    if(columns.isEmpty())throw new SQLException("Tablo görünmüyor veya sütun yetkisi yok","42501");
    return Map.of("schema",schema,"table",table,"columns",columns,"primaryKeys",objects(md.getPrimaryKeys(c.getCatalog(),schema,table)),"indexes",engine.equals("mssql")?sqlServerIndexes(c,schema,table):objects(md.getIndexInfo(c.getCatalog(),engine.equals("oracle")?q(schema,engine):schema,engine.equals("oracle")?q(table,engine):table,false,true)),"foreignKeys",objects(md.getImportedKeys(c.getCatalog(),schema,table)),"constraints",constraints(c,engine,schema,table));
  }
  static List<Map<String,Object>> sqlServerIndexes(Connection c,String schema,String table)throws SQLException {
    String sql="SELECT i.name AS index_name,c.name AS column_name,ic.key_ordinal AS ordinal_position,CASE WHEN i.is_unique=1 THEN 'false' ELSE 'true' END AS non_unique,i.filter_definition AS filter_condition,i.has_filter,ic.is_included_column,i.is_disabled,i.type_desc,ic.is_descending_key FROM sys.indexes i JOIN sys.objects t ON t.object_id=i.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id WHERE s.name=? AND t.name=? AND t.type IN ('U','V') AND i.index_id>0 ORDER BY i.name,ic.index_column_id";
    try(PreparedStatement st=c.prepareStatement(sql)){st.setQueryTimeout(5);st.setString(1,schema);st.setString(2,table);return objects(st.executeQuery());}
  }
  static List<Map<String,Object>> constraints(Connection c,String engine,String schema,String table)throws SQLException{
    String sql=engine.equals("oracle")?"SELECT constraint_name,constraint_type,status,deferrable,deferred,validated FROM all_constraints WHERE owner=? AND table_name=?":"SELECT constraint_name,constraint_type FROM information_schema.table_constraints WHERE table_schema=? AND table_name=?";
    try(PreparedStatement st=c.prepareStatement(sql)){st.setQueryTimeout(5);st.setString(1,schema);st.setString(2,table);return objects(st.executeQuery());}
  }
  static Object routines(Connection c,JsonObject p)throws SQLException{
    var md=c.getMetaData();String schema=Main.str(p,"schema",null),name=Main.str(p,"routine","");
    if(!name.isEmpty())return Map.of("parameters",objects(md.getProcedureColumns(c.getCatalog(),schema,pattern(md,name),"%")));
    return objects(md.getProcedures(c.getCatalog(),schema,"%"));
  }
  static Map<String,Object> browse(Connection c,String engine,JsonObject p)throws SQLException{
    String schema=Main.str(p,"schema",""),table=Main.str(p,"table","");var detail=detail(c,engine,schema,table);var columns=(List<Map<String,Object>>)detail.get("columns");Set<String> names=new HashSet<>();for(var col:columns)names.add((String)col.get("column_name"));
    var keys=(List<Map<String,Object>>)detail.get("primaryKeys");keys.sort(Comparator.comparingInt(k->Integer.parseInt((String)k.get("key_seq"))));
    List<String> unique=keys.stream().map(k->(String)k.get("column_name")).toList();
    if(unique.isEmpty()){
      var groups=new TreeMap<String,List<Map<String,Object>>>();for(var i:(List<Map<String,Object>>)detail.get("indexes"))if(i.get("column_name")!=null&&i.get("filter_condition")==null&&!Set.of("true","1").contains(Objects.toString(i.get("has_filter")))&&!Set.of("true","1").contains(Objects.toString(i.get("is_included_column")))&&!Set.of("true","1").contains(Objects.toString(i.get("is_disabled")))&&Set.of("false","0").contains(Objects.toString(i.get("non_unique"))))groups.computeIfAbsent(Objects.toString(i.get("index_name")),k->new ArrayList<>()).add(i);
      for(var list:groups.values()){list.sort(Comparator.comparingInt(k->Integer.parseInt((String)k.get("ordinal_position"))));var candidate=list.stream().map(k->(String)k.get("column_name")).toList();if(candidate.stream().allMatch(n->columns.stream().anyMatch(col->n.equals(col.get("column_name"))&&"0".equals(col.get("nullable"))))){unique=candidate;break;}}
    }
    int size=Main.integer(p,"pageSize",100);if(!Set.of(100,500,1000).contains(size))throw new IllegalArgumentException("Sayfa boyutu 100, 500 veya 1000 olmalı");
    List<String> order=new ArrayList<>();String sort=Main.str(p,"sort","");if(!sort.isEmpty()){if(!names.contains(sort))throw new IllegalArgumentException("Sıralama sütunu bulunamadı");order.add(sort);}for(String k:unique)if(!order.contains(k))order.add(k);
    if(order.stream().anyMatch(n->columns.stream().anyMatch(col->n.equals(col.get("column_name"))&&Set.of(Types.BLOB,Types.CLOB,Types.NCLOB,Types.BINARY,Types.VARBINARY,Types.LONGVARBINARY,Types.ARRAY,Types.SQLXML,Types.ROWID).contains(Integer.parseInt((String)col.get("data_type"))))))unique=List.of();
    if(order.stream().anyMatch(n->columns.stream().anyMatch(col->n.equals(col.get("column_name"))&&(Set.of(Types.LONGVARCHAR,Types.LONGNVARCHAR).contains(RowEditor.number(col,"data_type"))||(RowEditor.TEXT.contains(RowEditor.number(col,"data_type"))&&(RowEditor.number(col,"column_size")<=0||RowEditor.number(col,"column_size")>2048))))))unique=List.of();
    String direction=Main.str(p,"direction","ASC");if(!Set.of("ASC","DESC").contains(direction))throw new IllegalArgumentException("Sıralama yönü geçersiz");
    // Nullable sort expressions do not have a stable keyset ordering in this version.
    if(!sort.isEmpty()&&columns.stream().anyMatch(col->sort.equals(col.get("column_name"))&&!"0".equals(col.get("nullable"))))unique=List.of();
    List<String> where=new ArrayList<>();List<JsonElement> bindings=new ArrayList<>();List<String> bindingColumns=new ArrayList<>();
    if(p.has("filters")&&p.getAsJsonArray("filters").size()>20)throw new IllegalArgumentException("En fazla 20 filtre");
    if(p.has("filters"))for(var value:p.getAsJsonArray("filters")){JsonObject f=value.getAsJsonObject();String col=Main.str(f,"column",""),op=Main.str(f,"operator","=");if(!names.contains(col)||!Set.of("=","<>",">",">=","<","<=","LIKE","IS NULL","IS NOT NULL").contains(op))throw new IllegalArgumentException("Filtre geçersiz");where.add(q(col,engine)+" "+op+(op.startsWith("IS ")?"":" ?"));if(!op.startsWith("IS ")){bindings.add(f.get("value"));bindingColumns.add(col);}}
    JsonArray after=p.has("after")&&!p.get("after").isJsonNull()?p.getAsJsonArray("after"):null;
    if(after!=null){if(unique.isEmpty()||after.size()!=order.size())throw new IllegalArgumentException("Kararlı devam anahtarı yok");List<String> branches=new ArrayList<>();for(int i=0;i<order.size();i++){List<String> terms=new ArrayList<>();for(int j=0;j<i;j++){terms.add(q(order.get(j),engine)+" = ?");bindings.add(after.get(j));bindingColumns.add(order.get(j));}terms.add(q(order.get(i),engine)+(direction.equals("ASC")?" > ?":" < ?"));bindings.add(after.get(i));bindingColumns.add(order.get(i));branches.add("("+String.join(" AND ",terms)+")");}where.add("("+String.join(" OR ",branches)+")");}
    String sql="SELECT "+(engine.equals("mssql")?"TOP ("+(size+1)+") ":"")+"* FROM "+q(schema,engine)+"."+q(table,engine)+(where.isEmpty()?"":" WHERE "+String.join(" AND ",where))+(order.isEmpty()?"":" ORDER BY "+String.join(",",order.stream().map(n->q(n,engine)+" "+direction).toList()))+(engine.equals("postgresql")?" LIMIT "+(size+1):engine.equals("oracle")?" FETCH FIRST "+(size+1)+" ROWS ONLY":"");
    try(PreparedStatement st=c.prepareStatement(sql)){st.setQueryTimeout(15);for(int i=0;i<bindings.size();i++){
        JsonElement v=bindings.get(i);String column=bindingColumns.get(i);var info=columns.stream().filter(col->column.equals(col.get("column_name"))).findFirst().orElseThrow();int type=Integer.parseInt((String)info.get("data_type"));
        if(v==null||v.isJsonNull()){st.setNull(i+1,type);continue;}
        if(v.isJsonObject()){if(!Main.str(v.getAsJsonObject(),"kind","").equals("decimal"))throw new IllegalArgumentException("Filtre tipi geçersiz");v=v.getAsJsonObject().get("value");}
        String text=v.getAsString();
        if(Set.of(Types.NUMERIC,Types.DECIMAL,Types.BIGINT,Types.INTEGER,Types.SMALLINT,Types.TINYINT,Types.FLOAT,Types.DOUBLE,Types.REAL).contains(type))st.setBigDecimal(i+1,new java.math.BigDecimal(text));
        else if(type==Types.TIMESTAMP)st.setTimestamp(i+1,Timestamp.valueOf(text.replace('T',' ')));
        else if(type==Types.DATE&&text.length()>10)st.setTimestamp(i+1,Timestamp.valueOf(text.replace('T',' ')));
        else if(type==Types.DATE)st.setDate(i+1,java.sql.Date.valueOf(text));
        else if(type==Types.OTHER&&engine.equals("postgresql"))st.setObject(i+1,text,Types.OTHER);
        else if(type==Types.TIMESTAMP_WITH_TIMEZONE)st.setObject(i+1,java.time.OffsetDateTime.parse(text));
        else st.setString(i+1,text);
      }
      try(ResultSet rs=st.executeQuery()){Map<String,Object> result=new LinkedHashMap<>(Main.rows(rs,size,5_000_000));var rows=(List<List<Object>>)result.get("rows");var resultCols=(List<Map<String,Object>>)result.get("columns");List<Object> next=new ArrayList<>();boolean stable=!unique.isEmpty();if(stable&&!rows.isEmpty())for(String col:order){int index=-1;for(int i=0;i<resultCols.size();i++)if(col.equals(resultCols.get(i).get("name")))index=i;next.add(index<0?null:rows.get(rows.size()-1).get(index));}result.put("next",stable&&Boolean.TRUE.equals(result.get("truncated"))?next:null);result.put("stable",stable);result.put("detail",detail);result.put("largeValues",LobValue.support(detail));result.put("editing",RowEditor.support(c,engine,schema,table,detail));return result;}
    }
  }
}
