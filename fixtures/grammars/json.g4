grammar JSON;

// Parser rules
json : value ;

value
    : STRING
    | NUMBER
    | obj
    | arr
    | 'true'
    | 'false'
    | 'null'
    ;

obj
    : '{' pair (',' pair)* '}'
    | '{' '}'
    ;

pair : STRING ':' value ;

arr
    : '[' value (',' value)* ']'
    | '[' ']'
    ;

// Lexer rules
STRING : '"' (~["\\\r\n] | '\\' .)* '"' ;

NUMBER : '-'? [0-9]+ ('.' [0-9]+)? ;

WS : [ \t\r\n]+ -> skip ;
