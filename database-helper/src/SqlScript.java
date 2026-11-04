import java.util.*;
public final class SqlScript {
  private SqlScript(){}
  /** Split only at lexical statement boundaries; SQL Server keeps its batch scope. */
  static List<String> split(String sql,String engine){
    List<String> result=new ArrayList<>();StringBuilder b=new StringBuilder();String dollar=null;char quote=0;int comment=0;boolean lineComment=false;
    boolean oracleBlock=engine.equals("oracle")&&sql.stripLeading().matches("(?is)^(declare\\b|begin\\b|create\\s+(or\\s+replace\\s+)?(procedure|function|package|trigger|type)\\b).*");
    for(int i=0;i<sql.length();i++){
      char c=sql.charAt(i),n=i+1<sql.length()?sql.charAt(i+1):0;
      if(!lineComment&&comment==0&&quote==0&&dollar==null&&(i==0||sql.charAt(i-1)=='\n')){
        int end=sql.indexOf('\n',i);if(end<0)end=sql.length();String line=sql.substring(i,end).trim();
        if((engine.equals("mssql")&&line.matches("(?i)GO\\s*(--.*)?"))||(engine.equals("oracle")&&line.equals("/"))){add(result,b);i=end;oracleBlock=false;continue;}
      }
      if(lineComment){b.append(c);if(c=='\n')lineComment=false;continue;}
      if(comment>0){b.append(c);if(c=='/'&&n=='*'){comment++;b.append(n);i++;}else if(c=='*'&&n=='/'){comment--;b.append(n);i++;}continue;}
      if(dollar!=null){if(sql.startsWith(dollar,i)){b.append(dollar);i+=dollar.length()-1;dollar=null;}else b.append(c);continue;}
      if(quote!=0){b.append(c);if(c==quote){if(n==quote){b.append(n);i++;}else quote=0;}continue;}
      if(c=='-'&&n=='-'){lineComment=true;b.append(c).append(n);i++;continue;}
      if(c=='/'&&n=='*'){comment=1;b.append(c).append(n);i++;continue;}
      int endQuote=SqlLex.specialQuoteEnd(sql,i,engine);if(endQuote>=0){b.append(sql,i,endQuote);i=endQuote-1;continue;}
      if(c=='\''||c=='"'||(c=='['&&engine.equals("mssql"))){quote=c=='['?']':c;b.append(c);continue;}
      if(c=='$'&&engine.equals("postgresql")){var m=java.util.regex.Pattern.compile("\\$(?:[A-Za-z_][A-Za-z_0-9]*)?\\$").matcher(sql.substring(i));if(m.lookingAt()){dollar=m.group();b.append(dollar);i+=dollar.length()-1;continue;}}
      if(c==';'&&engine.equals("oracle")&&isOracleBlock(b.toString()))oracleBlock=true;
      if(c==';'&&!engine.equals("mssql")&&!oracleBlock){add(result,b);continue;}b.append(c);
      if(engine.equals("oracle")&&c=='\n'&&b.toString().stripLeading().matches("(?is)^(declare\\b|begin\\b|create\\s+(or\\s+replace\\s+)?(procedure|function|package|trigger|type)\\b).*"))oracleBlock=true;
    }
    if(quote!=0||comment>0||dollar!=null)throw new IllegalArgumentException("SQL içinde kapanmamış alıntı veya yorum");
    add(result,b);return result;
  }
  static String leadingSql(String sql){
    int i=0,depth=0;while(i<sql.length()){
      if(Character.isWhitespace(sql.charAt(i))){i++;continue;}
      if(sql.startsWith("--",i)){int e=sql.indexOf('\n',i);i=e<0?sql.length():e+1;continue;}
      if(sql.startsWith("/*",i)){i+=2;depth=1;while(i<sql.length()&&depth>0){if(sql.startsWith("/*",i)){depth++;i+=2;}else if(sql.startsWith("*/",i)){depth--;i+=2;}else i++;}continue;}
      break;
    }return sql.substring(i);
  }
  static boolean isOracleBlock(String sql){return leadingSql(sql).matches("(?is)^(declare\\b|begin\\b|create\\s+(or\\s+replace\\s+)?(procedure|function|package|trigger|type)\\b).*");}
  static void add(List<String> result,StringBuilder b){String part=b.toString().trim();if(!leadingSql(part).isEmpty())result.add(part);b.setLength(0);}
}
