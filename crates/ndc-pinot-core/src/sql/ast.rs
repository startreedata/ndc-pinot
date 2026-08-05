use std::fmt;

use common::format::{escape_string_literal, quote_identifier};

/// A minimal Pinot-flavoured SQL AST.
///
/// This is deliberately small: with no relationships, no correlated
/// subqueries, and no struct-array columns to project through, a query
/// against a single Pinot collection only ever needs a flat
/// `SELECT ... FROM ... WHERE ... [GROUP BY ...] [ORDER BY ...] [LIMIT ... OFFSET ...]`.
#[derive(Debug, Clone)]
pub struct Ident {
    pub value: String,
}

impl Ident {
    pub fn new(value: impl Into<String>) -> Self {
        Ident {
            value: value.into(),
        }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", quote_identifier(&self.value))
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Number(String),
    SingleQuotedString(String),
    Boolean(bool),
    Null,
}

impl Value {
    /// Converts a JSON scalar (as found in a `ComparisonValue::Scalar` or a
    /// variable binding) into a SQL literal. JSON arrays/objects are rejected by
    /// the caller before reaching here, except when building an `IN (...)` list,
    /// which unpacks the array itself rather than calling this on it.
    pub fn try_from_json_scalar(value: &serde_json::Value) -> Result<Value, String> {
        match value {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
            serde_json::Value::Number(n) => Ok(Value::Number(n.to_string())),
            serde_json::Value::String(s) => Ok(Value::SingleQuotedString(s.clone())),
            other => Err(format!("expected a scalar JSON value, found: {other}")),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{n}"),
            Value::SingleQuotedString(s) => write!(f, "'{}'", escape_string_literal(s)),
            Value::Boolean(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Value::Null => write!(f, "NULL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Like,
    And,
    Or,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BinaryOperator::Eq => "=",
            BinaryOperator::NotEq => "!=",
            BinaryOperator::Lt => "<",
            BinaryOperator::LtEq => "<=",
            BinaryOperator::Gt => ">",
            BinaryOperator::GtEq => ">=",
            BinaryOperator::Like => "LIKE",
            BinaryOperator::And => "AND",
            BinaryOperator::Or => "OR",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub args: Vec<Expr>,
    pub distinct: bool,
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;
        if self.distinct {
            write!(f, "DISTINCT ")?;
        }
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{arg}")?;
        }
        write!(f, ")")
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(Ident),
    Value(Value),
    Function(Function),
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
    Not(Box<Expr>),
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),
    InList {
        expr: Box<Expr>,
        list: Vec<Expr>,
        negated: bool,
    },
    Nested(Box<Expr>),
    /// A raw, already-escaped SQL fragment - used for splicing native query text.
    Raw(String),
}

impl Expr {
    pub fn ident(name: impl Into<String>) -> Expr {
        Expr::Identifier(Ident::new(name))
    }

    pub fn and(self, other: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::And,
            right: Box::new(other),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Identifier(ident) => write!(f, "{ident}"),
            Expr::Value(value) => write!(f, "{value}"),
            Expr::Function(function) => write!(f, "{function}"),
            Expr::BinaryOp { left, op, right } => write!(f, "{left} {op} {right}"),
            Expr::Not(expr) => write!(f, "NOT {expr}"),
            Expr::IsNull(expr) => write!(f, "{expr} IS NULL"),
            Expr::IsNotNull(expr) => write!(f, "{expr} IS NOT NULL"),
            Expr::InList {
                expr,
                list,
                negated,
            } => {
                write!(f, "{expr} ")?;
                if *negated {
                    write!(f, "NOT ")?;
                }
                write!(f, "IN (")?;
                for (index, item) in list.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Expr::Nested(expr) => write!(f, "({expr})"),
            Expr::Raw(sql) => write!(f, "{sql}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SelectItem {
    UnnamedExpr(Expr),
    ExprWithAlias { expr: Expr, alias: Ident },
}

impl fmt::Display for SelectItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectItem::UnnamedExpr(expr) => write!(f, "{expr}"),
            SelectItem::ExprWithAlias { expr, alias } => write!(f, "{expr} AS {alias}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl fmt::Display for OrderDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        })
    }
}

#[derive(Debug, Clone)]
pub struct OrderByExpr {
    pub expr: Expr,
    pub direction: OrderDirection,
}

impl fmt::Display for OrderByExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.expr, self.direction)
    }
}

/// The table an outer `Query` selects from: either a real Pinot table, or a
/// parenthesized native-query SQL fragment used as a derived table.
#[derive(Debug, Clone)]
pub enum TableFactor {
    Table(Ident),
    /// A pre-rendered, already-substituted native query, wrapped as a derived table.
    Derived(String),
}

impl fmt::Display for TableFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableFactor::Table(ident) => write!(f, "{ident}"),
            TableFactor::Derived(sql) => write!(f, "({sql})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    pub select: Vec<SelectItem>,
    pub from: TableFactor,
    pub predicate: Option<Expr>,
    pub group_by: Vec<Expr>,
    pub order_by: Vec<OrderByExpr>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl Query {
    pub fn new(from: TableFactor) -> Self {
        Query {
            select: Vec::new(),
            from,
            predicate: None,
            group_by: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SELECT ")?;
        if self.select.is_empty() {
            write!(f, "*")?;
        } else {
            for (index, item) in self.select.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{item}")?;
            }
        }
        write!(f, " FROM {}", self.from)?;
        if let Some(predicate) = &self.predicate {
            write!(f, " WHERE {predicate}")?;
        }
        if !self.group_by.is_empty() {
            write!(f, " GROUP BY ")?;
            for (index, expr) in self.group_by.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{expr}")?;
            }
        }
        if !self.order_by.is_empty() {
            write!(f, " ORDER BY ")?;
            for (index, order) in self.order_by.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{order}")?;
            }
        }
        if let Some(limit) = self.limit {
            write!(f, " LIMIT {limit}")?;
        }
        if let Some(offset) = self.offset {
            write!(f, " OFFSET {offset}")?;
        }
        Ok(())
    }
}

/// A full statement, optionally preceded by Pinot `SET` query-option
/// statements (used to turn on `enableNullHandling`, needed for correct SQL
/// `NULL` semantics instead of Pinot's default-null-value sentinels).
#[derive(Debug, Clone)]
pub struct Statement {
    pub options: Vec<(String, String)>,
    pub query: Query,
    pub explain: bool,
}

impl Statement {
    pub fn new(query: Query) -> Self {
        Statement {
            options: vec![("enableNullHandling".to_string(), "true".to_string())],
            query,
            explain: false,
        }
    }

    pub fn explain(mut self) -> Self {
        self.explain = true;
        self
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.options {
            write!(f, "SET {key} = {value}; ")?;
        }
        if self.explain {
            write!(f, "EXPLAIN PLAN FOR ")?;
        }
        write!(f, "{}", self.query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_simple_select() {
        let mut query = Query::new(TableFactor::Table(Ident::new("orders")));
        query.select = vec![
            SelectItem::ExprWithAlias {
                expr: Expr::ident("orderId"),
                alias: Ident::new("orderId"),
            },
            SelectItem::ExprWithAlias {
                expr: Expr::ident("total"),
                alias: Ident::new("total"),
            },
        ];
        query.predicate = Some(Expr::BinaryOp {
            left: Box::new(Expr::ident("total")),
            op: BinaryOperator::Gt,
            right: Box::new(Expr::Value(Value::Number("100".into()))),
        });
        query.limit = Some(10);

        let statement = Statement::new(query);

        assert_eq!(
            statement.to_string(),
            "SET enableNullHandling = true; SELECT \"orderId\" AS \"orderId\", \"total\" AS \"total\" FROM \"orders\" WHERE \"total\" > 100 LIMIT 10"
        );
    }
}
