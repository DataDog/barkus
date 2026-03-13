use serde::Deserialize;

/// Schema context for SQL generation. Provides table/column/type information
/// so that generated SQL references valid identifiers and types.
#[derive(Debug, Clone, Deserialize)]
pub struct SqlContext {
    pub tables: Vec<Table>,
    #[serde(default)]
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Column {
    pub name: String,
    pub ty: SqlType,
    #[serde(default)]
    pub nullable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Function {
    pub name: String,
    pub args: Vec<SqlType>,
    pub ret: SqlType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqlType {
    Integer,
    Float,
    Text,
    Boolean,
    Timestamp,
    Blob,
    Custom(String),
}

impl SqlContext {
    /// Create a synthetic schema for zero-config usage.
    pub fn synthetic() -> Self {
        SqlContext {
            tables: vec![
                Table {
                    name: "users".into(),
                    columns: vec![
                        Column { name: "id".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "name".into(), ty: SqlType::Text, nullable: false },
                        Column { name: "email".into(), ty: SqlType::Text, nullable: true },
                        Column { name: "age".into(), ty: SqlType::Integer, nullable: true },
                        Column { name: "active".into(), ty: SqlType::Boolean, nullable: false },
                        Column { name: "created_at".into(), ty: SqlType::Timestamp, nullable: false },
                    ],
                },
                Table {
                    name: "orders".into(),
                    columns: vec![
                        Column { name: "id".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "user_id".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "product_id".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "quantity".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "total".into(), ty: SqlType::Float, nullable: false },
                        Column { name: "status".into(), ty: SqlType::Text, nullable: false },
                    ],
                },
                Table {
                    name: "products".into(),
                    columns: vec![
                        Column { name: "id".into(), ty: SqlType::Integer, nullable: false },
                        Column { name: "name".into(), ty: SqlType::Text, nullable: false },
                        Column { name: "price".into(), ty: SqlType::Float, nullable: false },
                        Column { name: "category".into(), ty: SqlType::Text, nullable: true },
                    ],
                },
            ],
            functions: vec![
                Function { name: "COUNT".into(), args: vec![SqlType::Integer], ret: SqlType::Integer },
                Function { name: "SUM".into(), args: vec![SqlType::Float], ret: SqlType::Float },
                Function { name: "MAX".into(), args: vec![SqlType::Integer], ret: SqlType::Integer },
                Function { name: "MIN".into(), args: vec![SqlType::Integer], ret: SqlType::Integer },
                Function { name: "AVG".into(), args: vec![SqlType::Float], ret: SqlType::Float },
            ],
        }
    }
}
