import com.google.gson.*;
import java.sql.*;
import java.time.OffsetDateTime;
import java.util.*;
import java.util.regex.*;

public final class ActionRunner {
  static Map<String,Object> run(Main.Session s,JsonObject p)throws Exception{
    JsonObject action=p.getAsJsonObject("action"),contract=p.getAsJsonObject("contract"),input=p.has("parameters")?p.getAsJsonObject("parameters"):new JsonObject();
    String engine=s.engine,id=Main.str(action,"id","");
    if(!Main.str(action,"risk","").equals("R0")||!Main.str(action,"engine","").equals(engine))throw new IllegalArgumentException("Motor/risk uyuşmazlığı");
    long start=System.nanoTime();String operationId=UUID.randomUUID().toString();
    Map<String,Object> result=new LinkedHashMap<>();result.put("operationId",operationId);result.put("actionId",id);result.put("observedAt",System.currentTimeMillis());
    int major=s.connection.getMetaData().getDatabaseMajorVersion();
    if(major<Main.integer(contract,"minMajor",0)||major>Main.integer(contract,"maxMajor",0)||!contract.get("license").isJsonNull())return outcome(result,"unsupported","Sürüm veya lisans koşulu sağlanmıyor",start);
    try {
    if(id.equals("MS-MON-020")){try(Statement st=s.connection.createStatement()){st.setQueryTimeout(5);try(ResultSet rs=st.executeQuery("SELECT actual_state_desc FROM sys.database_query_store_options")){if(!rs.next()||Set.of("OFF","ERROR").contains(rs.getString(1)))return outcome(result,"unsupported","Query Store bu veritabanında etkin değil",start);}}}
    JsonObject specs=contract.getAsJsonObject("parameters");if(!specs.keySet().containsAll(input.keySet()))throw new IllegalArgumentException("Bilinmeyen kontrol parametresi");
    if(id.equals("PG-MON-026")&&input.has("pg_stat_statements_view")&&Main.str(input.getAsJsonObject("pg_stat_statements_view"),"schema","").equals("__auto__")){
      try(Statement st=s.connection.createStatement()){st.setQueryTimeout(5);try(ResultSet rs=st.executeQuery("SELECT n.nspname FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='pg_stat_statements'")){if(!rs.next())throw new SQLException("pg_stat_statements kurulu değil","42P01");input.getAsJsonObject("pg_stat_statements_view").addProperty("schema",rs.getString(1));}}
    }
    JsonObject values=new JsonObject();
    for(var entry:specs.entrySet()){
      String name=entry.getKey();JsonObject spec=entry.getValue().getAsJsonObject();JsonElement value=input.has(name)?input.get(name):spec.get("default");String type=Main.str(spec,"type","text");
      if(value==null||value.isJsonNull()){if(!Main.bool(spec,"nullable",false))throw new IllegalArgumentException(name+": değer gerekli");values.add(name,JsonNull.INSTANCE);continue;}
      if(type.equals("integer")){long n;try{n=value.getAsBigDecimal().longValueExact();}catch(Exception e){throw new IllegalArgumentException(name+": tam sayı gerekli");}if(n<spec.get("min").getAsLong()||n>spec.get("max").getAsLong())throw new IllegalArgumentException(name+": sınır dışında");}
      else if(Set.of("columns","textArray").contains(type)){if(!value.isJsonArray()||value.getAsJsonArray().size()>100)throw new IllegalArgumentException(name+": en fazla 100 öğe");for(var item:value.getAsJsonArray())if(!item.isJsonPrimitive()||item.getAsString().length()>256)throw new IllegalArgumentException(name+": metin dizisi gerekli");}
      else if(type.equals("object")){if(!value.isJsonObject())throw new IllegalArgumentException(name+": nesne seçin");DbTools.q(Main.str(value.getAsJsonObject(),"schema",""),engine);DbTools.q(Main.str(value.getAsJsonObject(),"table",""),engine);}
      else{String text=value.getAsString();if(text.length()>Main.integer(spec,"maxLength",256)||text.indexOf('\0')>=0)throw new IllegalArgumentException(name+": geçersiz metin");if(spec.has("pattern")&&!text.matches(spec.get("pattern").getAsString()))throw new IllegalArgumentException(name+": biçim geçersiz");if(type.equals("enum")&&!spec.getAsJsonArray("values").contains(value))throw new IllegalArgumentException(name+": seçenek geçersiz");if(type.equals("datetime"))OffsetDateTime.parse(text);}
      values.add(name,value);
    }
    if(values.has("from_time")&&values.has("to_time")){var from=OffsetDateTime.parse(values.get("from_time").getAsString());var to=OffsetDateTime.parse(values.get("to_time").getAsString());long seconds=java.time.Duration.between(from,to).getSeconds();if(seconds<=0||seconds>7*86400)throw new IllegalArgumentException("Zaman aralığı 0–7 gün olmalı");}
    String sql=String.join("\n\n",action.getAsJsonArray("sql").asList().stream().map(JsonElement::getAsString).toList());
    Matcher tokens=Pattern.compile("\\{\\{(qualified|columns|ident|int):(\\w+)\\}\\}").matcher(sql);StringBuffer rendered=new StringBuffer();
    Map<String,Object> table=null;
    if(values.has("table")&&values.get("table").isJsonObject()){var t=values.getAsJsonObject("table");table=DbTools.detail(s.connection,engine,Main.str(t,"schema",""),Main.str(t,"table",""));}
    Set<String> columns=new HashSet<>();if(table!=null)for(var col:(List<Map<String,Object>>)table.get("columns"))columns.add((String)col.get("column_name"));
    while(tokens.find()){
      String type=tokens.group(1),name=tokens.group(2),replacement;JsonElement value=values.get(name);
      if(type.equals("qualified")){var object=value.getAsJsonObject();String schema=Main.str(object,"schema",""),nameValue=Main.str(object,"table","");DbTools.detail(s.connection,engine,schema,nameValue);if(name.equals("pg_stat_statements_view")&&!nameValue.equals("pg_stat_statements"))throw new IllegalArgumentException("pg_stat_statements görünümü seçilmeli");replacement=DbTools.q(schema,engine)+"."+DbTools.q(nameValue,engine);}
      else if(type.equals("columns")){List<String> chosen=new ArrayList<>();for(var col:value.getAsJsonArray()){if(!columns.contains(col.getAsString()))throw new IllegalArgumentException("Sütun bulunamadı");chosen.add(DbTools.q(col.getAsString(),engine));}if(chosen.isEmpty())throw new IllegalArgumentException("Sütun seçin");replacement=String.join(",",chosen);}
      else if(type.equals("ident")){String col=value.getAsString();if(!columns.contains(col))throw new IllegalArgumentException("Anahtar sütunu bulunamadı");var pk=(List<Map<String,Object>>)table.get("primaryKeys");if(pk.size()!=1||!col.equals(pk.get(0).get("column_name")))throw new IllegalArgumentException("Tek sütunlu primary key gerekli; tablo gezgininde birleşik anahtar kullanılabilir");replacement=DbTools.q(col,engine);}
      else replacement=Long.toString(value.getAsLong());
      tokens.appendReplacement(rendered,Matcher.quoteReplacement(replacement));
    }tokens.appendTail(rendered);
    sql=rendered.toString();Set<String> bound=new HashSet<>();for(String batch:SqlScript.split(sql,engine))bound.addAll(Bindings.parse(batch,engine).names());JsonObject binds=new JsonObject();for(String name:bound)binds.add(name,values.get(name));
    if(engine.equals("postgresql")){s.connection.setReadOnly(true);s.connection.setAutoCommit(false);}
    JsonObject query=new JsonObject();query.addProperty("op","query");query.addProperty("sessionId",p.get("sessionId").getAsString());query.addProperty("sql",sql);query.add("parameters",binds);query.addProperty("timeout",Math.min(30,Main.integer(action,"timeout_seconds",10)));query.addProperty("maxRows",1000);
    try{
      var response=(Map<String,Object>)Main.execute(query);result.putAll(response);boolean partial=false,empty=true;for(var r:(List<Map<String,Object>>)response.get("results")){if(Boolean.TRUE.equals(r.get("truncated")))partial=true;if(r.get("rows") instanceof List<?> rows&&!rows.isEmpty())empty=false;}
      result.put("status",partial?"partial":empty?"unknown":"ok");result.put("state","succeeded");return result;
    }catch(SQLException e){return outcome(result,DbTools.classify(e),e.getMessage(),start);}finally{if(engine.equals("postgresql")){s.connection.rollback();s.connection.setAutoCommit(true);s.connection.setReadOnly(false);}}
    } catch(SQLException e){return outcome(result,DbTools.classify(e),e.getMessage(),start);}
  }
  static Map<String,Object> outcome(Map<String,Object> r,String status,String message,long start){r.put("status",status);r.put("state",status.equals("unknown_outcome")?status:"failed");r.put("message",message);r.put("results",List.of());r.put("elapsedMs",(System.nanoTime()-start)/1_000_000);return r;}
}
