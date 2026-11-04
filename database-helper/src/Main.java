import com.google.gson.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.sql.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

/** Private newline-delimited RPC. No listener, credentials in arguments, or SQL logging. */
public final class Main {
  static final Gson JSON=new GsonBuilder().serializeNulls().create();
  static final Map<String,Session> sessions=new ConcurrentHashMap<>();
  static final ExecutorService workers=Executors.newVirtualThreadPerTaskExecutor();
  static final Semaphore connectionSlots=new Semaphore(32);
  static final class Session {
    final Connection connection; final String engine; final JsonObject profile; final AtomicBoolean busy=new AtomicBoolean(); final AtomicBoolean cancelled=new AtomicBoolean(); final AtomicBoolean snapshotBusy=new AtomicBoolean();
    final RowEditor.State edits=new RowEditor.State();
    final ChangePlans.State maintenance=new ChangePlans.State();
    final Provisioning.State provisioning=new Provisioning.State();
    final PgExtensions.State extensions=new PgExtensions.State();
    final RowInsert.State inserts=new RowInsert.State();
    final RowDelete.State deletions=new RowDelete.State();
    volatile Statement statement; volatile boolean transaction=false; volatile String operationId="",jobState="succeeded";
    Session(Connection c,String e,JsonObject p){connection=c;engine=e;profile=p.deepCopy();}
  }
  static String str(JsonObject p,String k,String d){return p.has(k)&&!p.get(k).isJsonNull()?p.get(k).getAsString():d;}
  static int integer(JsonObject p,String k,int d){return p.has(k)?p.get(k).getAsInt():d;}
  static boolean bool(JsonObject p,String k,boolean d){return p.has(k)?p.get(k).getAsBoolean():d;}
  static boolean transactionOpen(Session s)throws SQLException{
    if(s.connection instanceof org.postgresql.core.BaseConnection pg)return pg.getTransactionState()!=org.postgresql.core.TransactionState.IDLE;
    if(s.engine.equals("mssql")){try(Statement statement=s.connection.createStatement()){statement.setQueryTimeout(3);try(ResultSet rs=statement.executeQuery("SELECT @@TRANCOUNT")){return rs.next()&&rs.getInt(1)>0;}}}
    if(s.engine.equals("oracle")){try(CallableStatement st=s.connection.prepareCall("BEGIN ? := DBMS_TRANSACTION.LOCAL_TRANSACTION_ID(FALSE); END;")){st.registerOutParameter(1,Types.VARCHAR);st.setQueryTimeout(3);st.execute();return st.getString(1)!=null;}}
    return s.transaction;
  }
  static synchronized void reply(JsonObject request,Object value,Throwable error){
    JsonObject out=new JsonObject();out.add("id",request.get("id"));
    if(error==null)out.add("result",JSON.toJsonTree(value));else{
      JsonObject problem=new JsonObject();problem.addProperty("message",error.getMessage());
      problem.addProperty("operationId",str(request,"operationId",""));problem.addProperty("state",error instanceof SQLException sql?DbTools.classify(sql):"failed");
      if(error instanceof SQLException s){problem.addProperty("sqlState",s.getSQLState());problem.addProperty("vendorCode",s.getErrorCode());}
      out.add("error",problem);
    }
    System.out.println(JSON.toJson(out));System.out.flush();
  }
  static Connection connect(JsonObject p)throws Exception{
    String engine=str(p,"engine",""),logicalHost=str(p,"host","localhost"),host=str(p,"transportHost",logicalHost),db=str(p,"database","");
    if(!host.matches("[a-zA-Z0-9_.:-]+"))throw new IllegalArgumentException("Geçersiz sunucu adı");
    int port=integer(p,"transportPort",integer(p,"port",engine.equals("postgresql")?5432:engine.equals("mssql")?1433:1521));
    if(port<1||port>65535)throw new IllegalArgumentException("Geçersiz port");
    Properties props=new Properties();props.setProperty("user",str(p,"username",""));props.setProperty("password",str(p,"password",""));
    boolean tls=!str(p,"tls","verify-full").equals("disabled-local");String url;
    if(!tls&&!Set.of("localhost","127.0.0.1","::1").contains(logicalHost))throw new IllegalArgumentException("TLS yalnız yerel laboratuvarda kapatılabilir");
    switch(engine){
      case "postgresql" -> {url="jdbc:postgresql://"+host+":"+port+"/";props.setProperty("PGDBNAME",db);props.setProperty("sslmode",tls?"verify-full":"disable");props.setProperty("connectTimeout","10");props.setProperty("ApplicationName","TermTerm Database");if(p.has("caPath")&&!str(p,"caPath","").isBlank())props.setProperty("sslrootcert",str(p,"caPath",""));}
      case "mssql" -> {url="jdbc:sqlserver://"+host+":"+port;props.setProperty("databaseName",db);props.setProperty("encrypt",tls?"true":"false");props.setProperty("trustServerCertificate","false");props.setProperty("loginTimeout","10");props.setProperty("applicationName","TermTerm Database");props.setProperty("cancelQueryTimeout","5");}
      case "oracle" -> {if(!db.matches("[a-zA-Z0-9_.-]+"))throw new IllegalArgumentException("Geçersiz Oracle service adı");url="jdbc:oracle:thin:@"+(tls?"tcps":"tcp")+"://"+host+":"+port+"/"+db;props.setProperty("oracle.net.ssl_server_dn_match","true");props.setProperty("oracle.net.CONNECT_TIMEOUT","10000");String role=str(p,"oracleRole","normal");if(!Set.of("normal","sysdba").contains(role))throw new IllegalArgumentException("Geçersiz Oracle bağlantı rolü");if(role.equals("sysdba"))props.setProperty("internal_logon","sysdba");}
      default -> throw new IllegalArgumentException("Desteklenmeyen motor");
    }
    if(!host.equals(logicalHost)&&tls){
      if(engine.equals("postgresql")){props.setProperty("sslhostnameverifier","TargetHostnameVerifier");TargetHostnameVerifier.target.set(logicalHost);}
      if(engine.equals("mssql"))props.setProperty("hostNameInCertificate",logicalHost);
      if(engine.equals("oracle")){String dn=str(p,"serverDn","");if(dn.isBlank())throw new IllegalArgumentException("Oracle TLS tünelinde sunucu sertifikasının DN alanı gerekli");props.setProperty("oracle.net.ssl_server_cert_dn",dn);}
    }
    String ca=str(p,"caPath","");
    if(tls&&!ca.isBlank()&&engine.equals("mssql")){props.setProperty("trustManagerClass","PemTrust");props.setProperty("trustManagerConstructorArg",JSON.toJson(Map.of("path",ca,"host",logicalHost)));}
    try{
      if(engine.equals("oracle")&&tls&&!ca.isBlank()){var source=new oracle.jdbc.pool.OracleDataSource();source.setURL(url);source.setConnectionProperties(props);source.setSSLContext(PemTrust.context(ca));return source.getConnection();}
      return DriverManager.getConnection(url,props);
    }finally{TargetHostnameVerifier.target.remove();}
  }
  static Object execute(JsonObject p)throws Exception{
    String op=str(p,"op",""),id=str(p,"sessionId","");
    if(op.equals("open")){
      if(!connectionSlots.tryAcquire())throw new IllegalStateException("En fazla 32 SQL bağlantısı; kullanılmayan sekmeleri kapatın");
      Connection c=null;
      try{c=connect(p.getAsJsonObject("profile"));var capabilities=DbTools.capabilities(c,str(p.getAsJsonObject("profile"),"engine",""));c.setAutoCommit(bool(p.getAsJsonObject("profile"),"autoCommit",true));c.setNetworkTimeout(workers,45000);id=UUID.randomUUID().toString();Session s=new Session(c,str(p.getAsJsonObject("profile"),"engine",""),p.getAsJsonObject("profile"));
        var info=Map.of("sessionId",id,"engine",s.engine,"version",c.getMetaData().getDatabaseProductVersion(),"database",Optional.ofNullable(c.getCatalog()).orElse(""),"autoCommit",c.getAutoCommit(),"capabilities",capabilities);sessions.put(id,s);return info;
      }catch(Exception e){if(c!=null)try{c.close();}catch(Exception ignored){}connectionSlots.release();throw e;}
    }
    Session s=sessions.get(id);if(s==null)throw new IllegalArgumentException("Bağlantı kapalı");
    if(op.equals("cancel")){if(p.has("operationId")&&!str(p,"operationId","").equals(s.operationId))throw new IllegalArgumentException("İşlem değişti; eski iptal atıldı");s.cancelled.set(true);Statement st=s.statement;if(st!=null)st.cancel();return Map.of("requested",true);}
    if(op.equals("close")){if(sessions.remove(id)!=null){RowEditor.close(s);ChangePlans.close(s);Provisioning.close(s);PgExtensions.close(s);RowDelete.close(s);RowInsert.close(s);try{s.connection.close();}finally{connectionSlots.release();}}return Map.of("closed",true);}
    if(Set.of("rowInsertMetadata","rowInsertPreview","rowInsertApply","rowInsertDiscard").contains(op))return RowInsert.execute(s,p);
    if(Set.of("rowDeletePreview","rowDeleteApply","rowDeleteDiscard").contains(op))return RowDelete.execute(s,p);
    if(Set.of("extensionEnvironment","extensionPreview","extensionApply","extensionDiscard").contains(op))return PgExtensions.execute(s,p);
    if(op.equals("provisionInspect"))return ProvisionInspection.inspect(s,p);
    if(Set.of("provisionPreview","provisionApply","provisionDiscard").contains(op))return Provisioning.execute(s,p);
    if(Set.of("changeOptions","changePreview","changeApply","changeDiscard").contains(op))return ChangePlans.execute(s,p);
    if(Set.of("rowEditOpen","rowEditPreview","rowEditApply","rowEditDiscard").contains(op))return RowEditor.execute(s,p);
    if(op.equals("schemaSnapshot"))return SchemaSnapshot.capture(s,p);
    if(op.equals("check"))return ActionRunner.run(s,p);
    if(Set.of("capabilities","detail","browse","routines","plan","metadataOptions","evidence","lobRead").contains(op)){
      if(!connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırına ulaşıldı");
      try(Connection metadata=connect(s.profile)){metadata.setNetworkTimeout(workers,20000);
        return switch(op){
          case "capabilities" -> DbTools.capabilities(metadata,s.engine);
          case "metadataOptions" -> MetadataOptions.read(metadata,s.engine,p);
          case "evidence" -> DatabaseEvidence.read(metadata,s.engine,p);
          case "lobRead" -> LobValue.read(metadata,s.engine,p);
          case "detail" -> DbTools.detail(metadata,s.engine,str(p,"schema",""),str(p,"table",""));
          case "browse" -> DbTools.browse(metadata,s.engine,p);
          case "routines" -> DbTools.routines(metadata,p);
          default -> Explain.run(metadata,s.engine,p);
        };
      }finally{connectionSlots.release();}
    }
    if(Set.of("schemas","tables","columns").contains(op)){
      if(!connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırına ulaşıldı");
      try(Connection metadata=connect(s.profile)){
        metadata.setNetworkTimeout(workers,15000);
        DatabaseMetaData md=metadata.getMetaData();
        try(ResultSet rs=op.equals("schemas")?md.getSchemas():op.equals("tables")?md.getTables(metadata.getCatalog(),str(p,"schema",null),"%",new String[]{"TABLE","VIEW"}):md.getColumns(metadata.getCatalog(),str(p,"schema",null),str(p,"table",""),"%")){
          return rows(rs,1000,2_000_000);
        }
      }finally{connectionSlots.release();}
    }
    if(!s.busy.compareAndSet(false,true))throw new IllegalStateException("Bu sorgu sekmesinde işlem devam ediyor");
    try{
      switch(op){
        case "status": {s.transaction=transactionOpen(s);return Map.of("transaction",s.transaction,"autoCommit",s.connection.getAutoCommit(),"operationId",s.operationId,"state",s.jobState);}
        case "transaction": {
          String action=str(p,"action","");
          if(action.equals("commit")||action.equals("rollback")){
            if(s.connection.getAutoCommit()){try(Statement st=s.connection.createStatement()){st.setQueryTimeout(10);st.execute(action.equals("commit")?"COMMIT":"ROLLBACK");}}else if(action.equals("commit"))s.connection.commit();else s.connection.rollback();
          }else if(action.equals("autocommit")){
            if(transactionOpen(s))throw new IllegalStateException("Önce Commit veya Rollback seçin");s.connection.setAutoCommit(bool(p,"enabled",true));
          }else throw new IllegalArgumentException("Geçersiz transaction işlemi");
          s.transaction=false;s.transaction=transactionOpen(s);return Map.of("autoCommit",s.connection.getAutoCommit(),"transaction",s.transaction);
        }
        case "schemas": {try(ResultSet rs=s.connection.getMetaData().getSchemas()){return rows(rs,1000,2_000_000);}}
        case "tables": {try(ResultSet rs=s.connection.getMetaData().getTables(s.connection.getCatalog(),str(p,"schema",null),"%",new String[]{"TABLE","VIEW"})){return rows(rs,1000,2_000_000);}}
        case "columns": {try(ResultSet rs=s.connection.getMetaData().getColumns(s.connection.getCatalog(),str(p,"schema",null),str(p,"table",""),"%")){return rows(rs,1000,2_000_000);}}
        case "query": {
          s.cancelled.set(false);s.operationId=str(p,"operationId",UUID.randomUUID().toString());s.jobState="running";
          String sql=str(p,"sql","");if(sql.isBlank()||sql.length()>1_000_000)throw new IllegalArgumentException("SQL boş veya çok büyük");
          int limit=Math.max(1,Math.min(integer(p,"maxRows",1000),1000));
          s.connection.setNetworkTimeout(workers,(Math.max(1,Math.min(integer(p,"timeout",30),600))+10)*1000);
          Budget resultBudget=new Budget(5_000_000);List<Object> results=new ArrayList<>();List<String> messages=new ArrayList<>();long start=System.nanoTime();
          var batches=SqlScript.split(sql,s.engine).stream().map(batch->Bindings.parse(batch,s.engine)).toList();
          JsonObject parameters=p.has("parameters")?p.getAsJsonObject("parameters"):new JsonObject();
          Set<String> used=new HashSet<>();for(var b:batches)used.addAll(b.names());
          if(!used.equals(parameters.keySet()))throw new IllegalArgumentException("SQL parametreleri ve girilen değerler eşleşmiyor: "+used);
          for(var batch:batches){
            if(s.cancelled.get())throw new SQLException("Script iptal edildi; sonraki batch çalıştırılmadı", "57014");
            try(Statement st=batch.names().isEmpty()?s.connection.createStatement():s.connection.prepareStatement(batch.sql())){
              s.statement=st;if(s.cancelled.get())throw new SQLException("Script iptal edildi", "57014");st.setQueryTimeout(Math.max(1,Math.min(integer(p,"timeout",30),600)));st.setMaxRows(limit+1);st.setFetchSize(Math.min(limit,100));
              if(st instanceof PreparedStatement prepared)Bindings.bind(prepared,batch,parameters,s.connection);
              s.transaction=!s.connection.getAutoCommit();boolean result=st instanceof PreparedStatement prepared?prepared.execute():st.execute(batch.source());
              do {
                if(result){try(ResultSet rs=st.getResultSet()){results.add(rows(rs,limit,resultBudget));}}
                else{int count=st.getUpdateCount();if(count==-1)break;results.add(Map.of("affectedRows",count));}
                if(results.size()>=32)throw new SQLException("En fazla 32 sonuç kümesi; scripti daha küçük parçalara ayırın");
                result=st.getMoreResults(Statement.CLOSE_CURRENT_RESULT);
              }while(true);
              for(SQLWarning warning=st.getWarnings();warning!=null&&messages.size()<100;warning=warning.getNextWarning()){String text=warning.getMessage();messages.add(text.substring(0,Math.min(text.length(),4096)));}
            }finally{s.statement=null;}
          }
          s.transaction=transactionOpen(s);
          s.jobState="succeeded";return Map.of("operationId",s.operationId,"state",s.jobState,"results",results,"messages",messages,"elapsedMs",(System.nanoTime()-start)/1_000_000,"transaction",s.transaction,"autoCommit",s.connection.getAutoCommit());
        }
        default: throw new IllegalArgumentException("Bilinmeyen işlem");
      }
    }catch(SQLException e){s.jobState=DbTools.classify(e);throw e;}catch(Exception e){s.jobState="failed";throw e;}finally{s.busy.set(false);}
  }
  static final class Budget { int remaining; Budget(int bytes){remaining=bytes;} }
  static Map<String,Object> rows(ResultSet rs,int limit,int byteLimit)throws SQLException{return rows(rs,limit,new Budget(byteLimit));}
  static Map<String,Object> rows(ResultSet rs,int limit,Budget budget)throws SQLException{
    return rows(rs,limit,budget,4096);
  }
  static Map<String,Object> rows(ResultSet rs,int limit,Budget budget,int fieldLimit)throws SQLException{
    var md=rs.getMetaData();List<Object> columns=new ArrayList<>(),data=new ArrayList<>();
    if(md.getColumnCount()>2048)throw new SQLException("En fazla 2048 sonuç sütunu");
    for(int i=1;i<=md.getColumnCount();i++)columns.add(Map.of("name",md.getColumnLabel(i),"type",md.getColumnTypeName(i)));
    boolean truncated=false;
    while(rs.next()){
      if(data.size()>=limit){truncated=true;break;}List<Object> row=new ArrayList<>();
      for(int i=1;i<=md.getColumnCount();i++){
        int type=md.getColumnType(i);Object value;
        if("uniqueidentifier".equalsIgnoreCase(md.getColumnTypeName(i)))value=rs.getString(i);
        else if(type==Types.BLOB||type==Types.BINARY||type==Types.VARBINARY||type==Types.LONGVARBINARY){try(InputStream stream=rs.getBinaryStream(i)){value=stream==null?null:"[binary — ayrı yükleme gerekli]";}catch(IOException e){throw new SQLException(e);}}
        else if(type==Types.CLOB||type==Types.NCLOB||type==Types.LONGVARCHAR||type==Types.LONGNVARCHAR||type==Types.VARCHAR||type==Types.NVARCHAR||type==Types.CHAR||type==Types.NCHAR){try(Reader r=rs.getCharacterStream(i)){if(r==null)value=null;else{char[] chars=new char[fieldLimit+1];int n=0,count;while(n<chars.length&&(count=r.read(chars,n,chars.length-n))>0)n+=count;value=new String(chars,0,Math.min(n,fieldLimit))+(n>fieldLimit?"… [alan kısaltıldı]":"");if(n>fieldLimit)truncated=true;}}catch(IOException e){throw new SQLException(e);}}
        else value=rs.getString(i);
        if(value instanceof String v){budget.remaining-=v.getBytes(StandardCharsets.UTF_8).length;if(v.length()>Math.max(65536,fieldLimit)){value=v.substring(0,Math.max(65536,fieldLimit))+"…";truncated=true;}}
        row.add(value);
      }
      if(budget.remaining<0){truncated=true;break;}data.add(row);
    }
    return Map.of("columns",columns,"rows",data,"truncated",truncated);
  }
  public static void main(String[] args)throws Exception{
    System.setOut(new PrintStream(new FileOutputStream(FileDescriptor.out),true,StandardCharsets.UTF_8));
    try(BufferedReader input=new BufferedReader(new InputStreamReader(System.in,StandardCharsets.UTF_8))){
      String line;while((line=input.readLine())!=null){final JsonObject request=JsonParser.parseString(line).getAsJsonObject();workers.submit(()->{try{reply(request,execute(request),null);}catch(Throwable e){reply(request,null,e);}});}
    }finally{for(Session s:sessions.values())try{s.connection.close();}catch(Exception ignored){}workers.shutdownNow();}
  }
}
