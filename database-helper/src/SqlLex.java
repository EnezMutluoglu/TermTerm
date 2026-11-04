public final class SqlLex {
  private SqlLex(){}
  /** Return exclusive end of Oracle alternative quotes or PostgreSQL E strings. */
  static int specialQuoteEnd(String text,int start,String engine){
    char c=text.charAt(start);if(start+2>=text.length()||text.charAt(start+1)!='\'')return -1;
    if(engine.equals("oracle")&&(c=='q'||c=='Q')){
      char open=text.charAt(start+2),close=switch(open){case '['->']';case '('->')';case '{'->'}';case '<'->'>';default->open;};
      int end=text.indexOf(""+close+'\'',start+3);if(end<0)throw new IllegalArgumentException("Oracle q alıntısı kapanmamış");return end+2;
    }
    if(engine.equals("postgresql")&&(c=='e'||c=='E')&&(start==0||!Character.isJavaIdentifierPart(text.charAt(start-1)))){
      for(int i=start+2;i<text.length();i++){
        if(text.charAt(i)=='\\'){i++;continue;}
        if(text.charAt(i)=='\''){if(i+1<text.length()&&text.charAt(i+1)=='\''){i++;continue;}return i+1;}
      }
      throw new IllegalArgumentException("PostgreSQL E alıntısı kapanmamış");
    }
    return -1;
  }
}
