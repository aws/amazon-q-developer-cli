export declare enum NodeType {
    Program = "program",
    AssignmentList = "assignment_list",
    Assignment = "assignment",
    VariableName = "variable_name",
    Subscript = "subscript",
    CompoundStatement = "compound_statement",
    Subshell = "subshell",
    Command = "command",
    Pipeline = "pipeline",
    List = "list",
    ProcessSubstitution = "process_substitution",
    Concatenation = "concatenation",
    Word = "word",
    String = "string",
    Expansion = "expansion",
    CommandSubstitution = "command_substitution",
    RawString = "raw_string",
    AnsiCString = "ansi_c_string",
    SimpleExpansion = "simple_expansion",
    SpecialExpansion = "special_expansion",
    ArithmeticExpansion = "arithmetic_expansion"
}
export type LiteralNode = BaseNode<NodeType.String> | BaseNode<NodeType.AnsiCString> | BaseNode<NodeType.RawString> | BaseNode<NodeType.CommandSubstitution> | BaseNode<NodeType.Concatenation> | BaseNode<NodeType.Expansion> | BaseNode<NodeType.ArithmeticExpansion> | BaseNode<NodeType.SimpleExpansion> | BaseNode<NodeType.SpecialExpansion> | BaseNode<NodeType.Word>;
export interface BaseNode<Type extends NodeType = NodeType> {
    text: string;
    innerText: string;
    startIndex: number;
    endIndex: number;
    complete: boolean;
    type: Type;
    children: BaseNode[];
}
export interface ListNode extends BaseNode {
    type: NodeType.List;
    operator: "||" | "&&" | "|" | "|&";
}
export interface AssignmentListNode extends BaseNode {
    type: NodeType.AssignmentList;
    children: [...AssignmentNode[], BaseNode<NodeType.Command>] | AssignmentNode[];
    hasCommand: boolean;
}
export interface AssignmentNode extends BaseNode {
    type: NodeType.Assignment;
    operator: "=" | "+=";
    name: BaseNode<NodeType.VariableName> | SubscriptNode;
    children: LiteralNode[];
}
export interface SubscriptNode extends BaseNode {
    type: NodeType.Subscript;
    name: BaseNode<NodeType.VariableName>;
    index: LiteralNode;
}
export declare const createTextNode: (str: string, startIndex: number, text: string) => BaseNode;
export declare const printTree: (root: BaseNode) => void;
export declare const parse: (str: string) => BaseNode;
