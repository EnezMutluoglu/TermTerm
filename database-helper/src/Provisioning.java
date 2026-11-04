import com.google.gson.*;
import java.sql.*;
import java.util.*;
import java.security.SecureRandom;
import java.util.concurrent.atomic.AtomicBoolean;

/** Fresh targets only. Passwords are generated at apply time, never retained in a plan. */
final class Provisioning {
  static final SecureRandom RANDOM=new SecureRandom();
  static final String[] CHARS={"ABCDEFGHIJKLMNOPQRSTUVWXYZ","abcdefghijklmnopqrstuvwxyz","0123456789","!#%+-_="};
  static final class State {final AtomicBoolean busy=new AtomicBoolean();volatile boolean closed;volatile Connection active;Draft draft;}
  record Input(String name,String locale,String oracleMode,String tablespace,int quotaMb,int pdbMaxMb){}
  record Step(String database,String label,String sql){}
  record Draft(String id,long created,Input input,String hash,List<Step> steps,String identity){}
  static String password(){String all=String.join("",CHARS);char[] chars=new char[21];for(int i=0;i<4;i++)chars[i]=CHARS[i].charAt(RANDOM.nextInt(CHARS[i].length()));for(int i=4;i<21;i++)chars[i]=all.charAt(RANDOM.nextInt(all.length()));for(int i=20;i>0;i--){int j=RANDOM.nextInt(i+1);char x=chars[i];chars[i]=chars[j];chars[j]=x;}return new String(chars);}
  static String lit(String value){return "'"+value.replace("'","''")+"'";}
  static String name(String value,String engine){return engine.equals("oracle")?value.toUpperCase(Locale.ROOT):value;}
  static String q(String value,String engine){return DbTools.q(name(value,engine),engine);}
  static List<String> names(Input in){return List.of(in.name,in.name+"_owner",in.name+"_app",in.name+"_developer",in.name+"_readonly",in.name+"_appuser");}
  static Input input(JsonObject p){
    String n=Main.str(p,"name","");if(!n.matches("[a-z][a-z0-9_]{0,47}")||n.startsWith("pg_")||n.startsWith("sys_")||n.startsWith("ora_"))throw new IllegalArgumentException("Ad: küçük harf, rakam, alt çizgi; harfle başlayan en fazla 48 karakter");
    String locale=Main.str(p,"locale","tr_TR.UTF-8"),mode=Main.str(p,"oracleMode","schema"),ts=Main.str(p,"tablespace","USERS");
    if(locale.length()>100||!Set.of("schema","pdb").contains(mode)||!ts.matches("[A-Za-z][A-Za-z0-9_]{0,29}"))throw new IllegalArgumentException("Geçersiz locale/PDB/tablespace seçimi");
    int quota=Main.integer(p,"quotaMb",1024),max=Main.integer(p,"pdbMaxMb",4096);if(quota<10||quota>1048576||max<2048||max>1048576||quota>max)throw new IllegalArgumentException("Quota/PDB sınırı geçersiz");
    return new Input(n,locale,mode,ts.toUpperCase(Locale.ROOT),quota,max);
  }
  static String scalar(Connection c,String sql,String... args)throws Exception {try(var st=c.prepareStatement(sql)){st.setQueryTimeout(10);for(int i=0;i<args.length;i++)st.setString(i+1,args[i]);try(var rs=st.executeQuery()){return rs.next()?rs.getString(1):null;}}}
  static void check(Connection c,String engine,Input in)throws Exception{
    int major=c.getMetaData().getDatabaseMajorVersion();
    if(!((engine.equals("postgresql")&&major==18)||(engine.equals("mssql")&&major==16)||(engine.equals("oracle")&&major==23)))throw new IllegalArgumentException("Otomatik kurulum bu motor/sürümde henüz doğrulanmadı (PG 18, SQL Server 2022, Oracle 23+ laboratuvarı)");
    if(engine.equals("postgresql")){
      if(!"t".equals(scalar(c,"SELECT rolsuper OR (rolcreatedb AND rolcreaterole) FROM pg_roles WHERE rolname=current_user")))throw new IllegalArgumentException("PostgreSQL CREATEDB ve CREATEROLE yetkileri gerekli; PaaS hesabında superuser zorunlu değil");
      if(scalar(c,"SELECT 1 FROM pg_database WHERE datname=?",in.name)!=null)throw new IllegalArgumentException("Veritabanı zaten var; mevcut hedef değiştirilmez");
      for(String n:names(in))if(scalar(c,"SELECT 1 FROM pg_roles WHERE rolname=?",n)!=null)throw new IllegalArgumentException("Rol zaten var: "+n);
      if(scalar(c,"SELECT 1 FROM pg_collation WHERE collprovider='c' AND lower(replace(collcollate,'-',''))=lower(replace(?,'-',''))",in.locale)==null&&!in.locale.equals("C")&&!in.locale.equals("C.UTF-8"))throw new IllegalArgumentException("Locale sunucu kataloğunda yok: "+in.locale+". Sunucuda kurulup pg_import_system_collations ile alınmalı; başka locale'e sessizce dönülmez.");
    }else if(engine.equals("mssql")){
      if(!"1".equals(scalar(c,"SELECT IS_SRVROLEMEMBER('sysadmin')")))throw new IllegalArgumentException("Bu geliştirme şablonu SQL Server sysadmin bağlantısı gerektiriyor");
      if(scalar(c,"SELECT 1 FROM sys.databases WHERE name=?",in.name)!=null)throw new IllegalArgumentException("Veritabanı zaten var");
      for(String suffix:List.of("_owner","_appuser"))if(scalar(c,"SELECT 1 FROM sys.server_principals WHERE name=?",in.name+suffix)!=null)throw new IllegalArgumentException("Login zaten var");
      if(!in.locale.matches("[A-Za-z0-9_]+")||scalar(c,"SELECT 1 FROM sys.fn_helpcollations() WHERE name=?",in.locale)==null)throw new IllegalArgumentException("SQL Server collation bulunamadı");
    }else{
      if(scalar(c,"SELECT 1 FROM session_roles WHERE role='DBA'")==null&&!"TRUE".equals(scalar(c,"SELECT SYS_CONTEXT('USERENV','ISDBA') FROM dual")))throw new IllegalArgumentException("Bu geliştirme şablonu Oracle DBA bağlantısı gerektiriyor");
      String container=scalar(c,"SELECT SYS_CONTEXT('USERENV','CON_NAME') FROM dual");
      if(in.oracleMode.equals("pdb")){
        if(!"CDB$ROOT".equals(container))throw new IllegalArgumentException("Yeni PDB için CDB$ROOT service bağlantısı seçin");
        if(in.name.length()>30)throw new IllegalArgumentException("PDB adı en fazla 30 karakter olmalı");
        if(!"TRUE".equals(scalar(c,"SELECT SYS_CONTEXT('USERENV','ISDBA') FROM dual")))throw new IllegalArgumentException("Yeni PDB oluşturup açmak için bağlantı rolü SYSDBA olmalı; DBA rolü tek başına yeterli değil");
        if(scalar(c,"SELECT 1 FROM v$pdbs WHERE name=?",name(in.name,engine))!=null)throw new IllegalArgumentException("PDB zaten var");
        if(Objects.toString(scalar(c,"SELECT value FROM v$parameter WHERE name='db_create_file_dest'"),"").isBlank())throw new IllegalArgumentException("Yeni PDB için sunucuda Oracle Managed Files (db_create_file_dest) gerekli; dosya yolu tahmin edilmez");
      }else{
        if("CDB$ROOT".equals(container)||"PDB$SEED".equals(container))throw new IllegalArgumentException("Schema için kullanılabilir bir PDB'ye bağlanın");
        if(scalar(c,"SELECT 1 FROM dba_tablespaces WHERE tablespace_name=? AND contents='PERMANENT' AND status='ONLINE'",in.tablespace)==null)throw new IllegalArgumentException("Kalıcı, çevrimiçi tablespace bulunamadı");
      }
      for(String n:names(in))if(scalar(c,"SELECT 1 FROM dba_users WHERE username=? UNION ALL SELECT 1 FROM dba_roles WHERE role=?",name(n,engine),name(n,engine))!=null)throw new IllegalArgumentException("Kullanıcı/rol zaten var: "+name(n,engine));
    }
  }
  static List<Step> steps(String e,Input in){
    var out=new ArrayList<Step>();String b=q(in.name,e),o=q(in.name+"_owner",e),a=q(in.name+"_app",e),d=q(in.name+"_developer",e),r=q(in.name+"_readonly",e),u=q(in.name+"_appuser",e);
    java.util.function.BiConsumer<String,String> admin=(label,sql)->out.add(new Step("@admin",label,sql));
    java.util.function.BiConsumer<String,String> target=(label,sql)->out.add(new Step("@target",label,sql));
    if(e.equals("postgresql")){
      admin.accept("Ana grup","CREATE ROLE "+b+" NOLOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS");
      admin.accept("Owner hesabı","CREATE ROLE "+o+" LOGIN INHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '<OWNER_PASSWORD>' IN ROLE "+b);
      admin.accept("Yönetici owner üyeliği","GRANT "+o+" TO CURRENT_USER WITH INHERIT TRUE, SET TRUE");
      for(String role:List.of(a,d,r))admin.accept("Yetki grubu","CREATE ROLE "+role+" NOLOGIN INHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS IN ROLE "+b);
      admin.accept("Uygulama hesabı","CREATE ROLE "+u+" LOGIN INHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '<APP_PASSWORD>' IN ROLE "+a);
      admin.accept("Veritabanı oluştur","CREATE DATABASE "+b+" LC_COLLATE="+lit(in.locale)+" LC_CTYPE="+lit(in.locale)+" LOCALE_PROVIDER=libc TEMPLATE=template0 ENCODING='UTF8' OWNER="+o+" ALLOW_CONNECTIONS=false");
      admin.accept("PUBLIC DB erişimini kapat","REVOKE CONNECT,TEMP ON DATABASE "+b+" FROM PUBLIC");admin.accept("Grup DB erişimi","GRANT CONNECT,TEMP ON DATABASE "+b+" TO "+b);
      admin.accept("DB bağlantısını aç","ALTER DATABASE "+b+" ALLOW_CONNECTIONS=true");
      target.accept("Owner yetki bağlamı","SET ROLE "+o);
      target.accept("Schema sahibi","ALTER SCHEMA public OWNER TO "+o);target.accept("PUBLIC schema erişimini kapat","REVOKE CREATE,USAGE ON SCHEMA public FROM PUBLIC");target.accept("Grup schema erişimi","GRANT USAGE ON SCHEMA public TO "+b);
      target.accept("Fonksiyon varsayılanını daralt","ALTER DEFAULT PRIVILEGES FOR ROLE "+o+" REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC");
      target.accept("Tip varsayılanını daralt","ALTER DEFAULT PRIVILEGES FOR ROLE "+o+" REVOKE USAGE ON TYPES FROM PUBLIC");
      for(String role:List.of(a,d,r)){
        boolean ro=role.equals(r);String prefix="ALTER DEFAULT PRIVILEGES FOR ROLE "+o+" GRANT ";
        target.accept("Gelecek tablolar",prefix+(ro?"SELECT":"SELECT,INSERT,UPDATE,DELETE")+" ON TABLES TO "+role);
        target.accept("Gelecek sequence'ler",prefix+(ro?"SELECT":"USAGE,SELECT,UPDATE")+" ON SEQUENCES TO "+role);
        if(!ro)target.accept("Gelecek fonksiyonlar",prefix+"EXECUTE ON FUNCTIONS TO "+role);
        target.accept("Gelecek tipler",prefix+"USAGE ON TYPES TO "+role);target.accept("Gelecek schema'lar",prefix+"USAGE ON SCHEMAS TO "+role);
      }
    }else if(e.equals("mssql")){
      admin.accept("Owner login","CREATE LOGIN "+o+" WITH PASSWORD='<OWNER_PASSWORD>',CHECK_POLICY=ON");admin.accept("Uygulama login","CREATE LOGIN "+u+" WITH PASSWORD='<APP_PASSWORD>',CHECK_POLICY=ON");
      admin.accept("Veritabanı oluştur","CREATE DATABASE "+b+" COLLATE "+in.locale);admin.accept("DB sahibi","ALTER AUTHORIZATION ON DATABASE::"+b+" TO "+o);
      for(String role:List.of(b,a,d,r))target.accept("Yetki grubu","CREATE ROLE "+role+" AUTHORIZATION dbo");
      for(String role:List.of(a,d,r))target.accept("Ana gruba üyelik","ALTER ROLE "+b+" ADD MEMBER "+role);
      target.accept("Uygulama DB kullanıcısı","CREATE USER "+u+" FOR LOGIN "+u+" WITH DEFAULT_SCHEMA=dbo");target.accept("App üyeliği","ALTER ROLE "+a+" ADD MEMBER "+u);
      target.accept("Guest bağlantısını kapat","REVOKE CONNECT FROM guest");target.accept("PUBLIC doğrudan bağlantı hakkını kaldır","REVOKE CONNECT FROM public");target.accept("Ana grup bağlantısı","GRANT CONNECT TO "+b);
      for(String role:List.of(a,d,r)){
        target.accept("Mevcut ve gelecek dbo tabloları","GRANT "+(role.equals(r)?"SELECT":"SELECT,INSERT,UPDATE,DELETE,EXECUTE,REFERENCES")+" ON SCHEMA::dbo TO "+role);
      }
      admin.accept("Owner varsayılan DB","ALTER LOGIN "+o+" WITH DEFAULT_DATABASE="+b);admin.accept("App varsayılan DB","ALTER LOGIN "+u+" WITH DEFAULT_DATABASE="+b);
    }else{
      if(in.oracleMode.equals("pdb")){
        admin.accept("PDB oluştur","CREATE PLUGGABLE DATABASE "+b+" ADMIN USER "+q(in.name+"_pdbadmin",e)+" IDENTIFIED BY \"<PDB_PASSWORD>\" ROLES=(DBA) DEFAULT TABLESPACE "+q(in.tablespace,e)+" DATAFILE SIZE "+Math.min(100,in.quotaMb)+"M AUTOEXTEND ON NEXT 10M MAXSIZE "+in.quotaMb+"M STORAGE (MAXSIZE "+in.pdbMaxMb+"M)");
        admin.accept("PDB aç","ALTER PLUGGABLE DATABASE "+b+" OPEN");admin.accept("PDB açılma tercihi","ALTER PLUGGABLE DATABASE "+b+" SAVE STATE");
      }
      for(String role:List.of(b,a,d,r))target.accept("Yetki grubu","CREATE ROLE "+role+" NOT IDENTIFIED");
      target.accept("Owner/schema hesabı","CREATE USER "+o+" IDENTIFIED BY \"<OWNER_PASSWORD>\" DEFAULT TABLESPACE "+q(in.tablespace,e)+" QUOTA "+in.quotaMb+"M ON "+q(in.tablespace,e));
      target.accept("Uygulama hesabı","CREATE USER "+u+" IDENTIFIED BY \"<APP_PASSWORD>\"");
      target.accept("Grup bağlantısı","GRANT CREATE SESSION TO "+b);target.accept("Owner schema DDL","GRANT CREATE TABLE,CREATE VIEW,CREATE SEQUENCE,CREATE PROCEDURE,CREATE TRIGGER,CREATE TYPE TO "+o);
      for(String role:List.of(o,a,d,r))target.accept("Ana grup üyeliği","GRANT "+b+" TO "+role);target.accept("App üyeliği","GRANT "+a+" TO "+u);
      for(String role:List.of(a,d,r)){
        target.accept("Mevcut/gelecek schema tabloları","GRANT "+(role.equals(r)?"READ ANY TABLE":"SELECT ANY TABLE,INSERT ANY TABLE,UPDATE ANY TABLE,DELETE ANY TABLE")+" ON SCHEMA "+o+" TO "+role);
        if(!role.equals(r)){target.accept("Schema routine'leri","GRANT EXECUTE ANY PROCEDURE,EXECUTE ANY TYPE,SELECT ANY SEQUENCE ON SCHEMA "+o+" TO "+role);}
      }
    }
    return out;
  }
  static void close(Main.Session s){State st=s.provisioning;st.closed=true;synchronized(st){st.draft=null;}Connection c=st.active;if(c!=null)try{c.abort(Main.workers);}catch(Exception ignored){}}
  static Object execute(Main.Session s,JsonObject p)throws Exception{
    String op=Main.str(p,"op","");Set<String> keys=new HashSet<>(Set.of("id","op","sessionId"));keys.addAll(op.equals("provisionPreview")?Set.of("name","locale","oracleMode","tablespace","quotaMb","pdbMaxMb"):Set.of("planId","hash"));if(!keys.containsAll(p.keySet()))throw new IllegalArgumentException("Beklenmeyen şablon alanı");
    State state=s.provisioning;if(!state.busy.compareAndSet(false,true))throw new IllegalStateException("Kurulum işlemi sürüyor");
    Connection c=null;boolean slot=false,started=false;String ownerPassword=null,appPassword=null,pdbPassword=null;int completed=0;Draft draft=null;
    try{
      if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
      if(op.equals("provisionDiscard")){synchronized(state){if(state.draft!=null&&state.draft.id.equals(Main.str(p,"planId","")))state.draft=null;}return Map.of("discarded",true);}
      if(op.equals("provisionApply"))synchronized(state){draft=state.draft;if(draft==null||ChangePlans.expired(draft.created,System.nanoTime())||!draft.id.equals(Main.str(p,"planId",""))||!draft.hash.equals(Main.str(p,"hash","")))throw new IllegalArgumentException("Plan değişti/süresi doldu; tekrar kullanılmaz");state.draft=null;}
      else synchronized(state){state.draft=null;}
      if(!Main.connectionSlots.tryAcquire())throw new IllegalStateException("Bağlantı sınırı");slot=true;c=Main.connect(s.profile);state.active=c;c.setAutoCommit(true);c.setNetworkTimeout(Main.workers,90000);
      Input in=op.equals("provisionPreview")?input(p):draft.input;check(c,s.engine,in);
      String identity=Main.JSON.toJson(List.of(s.engine,c.getMetaData().getDatabaseProductVersion(),c.getMetaData().getUserName(),Main.str(s.profile,"host",""),Main.integer(s.profile,"port",0),Main.str(s.profile,"database","")));
      if(op.equals("provisionPreview")){
        var steps=steps(s.engine,in);String hash=ChangePlans.hash(identity+Main.JSON.toJson(in)+Main.JSON.toJson(steps));draft=new Draft(UUID.randomUUID().toString(),System.nanoTime(),in,hash,steps,identity);
        synchronized(state){if(state.closed)throw new IllegalStateException("Bağlantı kapandı");state.draft=draft;}
        return Map.of("planId",draft.id,"hash",hash,"steps",steps,"names",names(in).stream().map(n->name(n,s.engine)).toList(),"host",Main.str(s.profile,"host",""),"database",s.engine.equals("oracle")&&in.oracleMode.equals("schema")?Main.str(s.profile,"database",""):in.name,"schema",s.engine.equals("oracle")?name(in.name+"_owner",s.engine):s.engine.equals("mssql")?"dbo":"public","expiresInSeconds",300);
      }
      if(!identity.equals(draft.identity))throw new IllegalArgumentException("Bağlantı kimliği değişti");
      ownerPassword=password();appPassword=password();if(s.engine.equals("oracle")&&in.oracleMode.equals("pdb"))pdbPassword=password();
      String location="@admin";
      for(Step step:draft.steps){
        if(state.closed)throw new IllegalStateException("Bağlantı kapandı");
        if(!step.database.equals(location)){
          if(s.engine.equals("oracle")){
            if(in.oracleMode.equals("pdb"))try(var st=c.createStatement()){st.setQueryTimeout(30);st.execute("ALTER SESSION SET CONTAINER="+q(step.database.equals("@target")?in.name:"CDB$ROOT",s.engine));}
          }else{c.close();JsonObject profile=s.profile.deepCopy();if(step.database.equals("@target"))profile.addProperty("database",in.name);c=Main.connect(profile);state.active=c;c.setAutoCommit(true);c.setNetworkTimeout(Main.workers,90000);}
          location=step.database;
        }
        String sql=step.sql.replace("<OWNER_PASSWORD>",ownerPassword).replace("<APP_PASSWORD>",appPassword);if(pdbPassword!=null)sql=sql.replace("<PDB_PASSWORD>",pdbPassword);
        try(var st=c.createStatement()){st.setQueryTimeout(60);started=true;st.execute(sql);}completed++;
      }
      String db=s.engine.equals("oracle")&&in.oracleMode.equals("schema")?Main.str(s.profile,"database",""):in.name;
      for(String suffix:List.of("_owner","_appuser")){
        JsonObject profile=s.profile.deepCopy();profile.remove("oracleRole");profile.addProperty("database",db);profile.addProperty("username",name(in.name+suffix,s.engine));profile.addProperty("password",suffix.equals("_owner")?ownerPassword:appPassword);
        try(Connection verify=Main.connect(profile)){verify.setNetworkTimeout(Main.workers,15000);if(!"1".equals(scalar(verify,s.engine.equals("oracle")?"SELECT 1 FROM dual":"SELECT 1")))throw new IllegalStateException("Hesap doğrulanamadı");}
      }
      return result(s,draft,"succeeded",completed,ownerPassword,appPassword,pdbPassword,"Kurulum ve iki hesabın gerçek girişi doğrulandı. Parolaları şimdi alın; tekrar gösterilmez.");
    }catch(Exception ex){
      if(op.equals("provisionApply")&&started)return result(s,draft,"unknown_outcome",completed,ownerPassword,appPassword,pdbPassword,"Kurulum bütünüyle doğrulanamadı. Tamamlanan adımlar korunur; otomatik silme/tekrar yok. Parolalar oluşturulmuş olabilecek hesaplara aittir. DBA tamamlanan adımları kontrol etmeli. SQL hata kodu: "+(ex instanceof SQLException se?Objects.toString(se.getSQLState(),"")+"/"+se.getErrorCode():"yerel/bağlantı"));
      throw ex;
    }finally{state.active=null;if(c!=null)try{c.close();}catch(Exception ignored){}if(slot)Main.connectionSlots.release();state.busy.set(false);}
  }
  static Object result(Main.Session s,Draft draft,String status,int completed,String owner,String app,String admin,String message){
    var secrets=new ArrayList<Object>();secrets.add(Map.of("username",name(draft.input.name+"_owner",s.engine),"password",owner));secrets.add(Map.of("username",name(draft.input.name+"_appuser",s.engine),"password",app));if(admin!=null)secrets.add(Map.of("username",name(draft.input.name+"_pdbadmin",s.engine),"password",admin));
    return Map.of("state",status,"message",message,"completedSteps",completed,"totalSteps",draft.steps.size(),"credentials",secrets,"database",s.engine.equals("oracle")&&draft.input.oracleMode.equals("schema")?Main.str(s.profile,"database",""):draft.input.name);
  }
}
