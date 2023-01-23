use crate::config;
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{sqlite::SqlitePool, Executor};
use std::collections::HashMap;
use std::{
    convert::Infallible,
    fmt::{Debug, Display},
    sync::Arc,
};
use teloxide::dispatching::dialogue::{Serializer, Storage};
use teloxide::types::ChatId;
use thiserror::Error;
use tracing::{instrument, trace};

/// A persistent dialogue storage based on [SQLite](https://www.sqlite.org/).
pub struct SqliteStorage<S> {
    pool: SqlitePool,
    serializer: S,
}

/// An error returned from [`SqliteStorage`].
#[derive(Debug, Error)]
pub enum SqliteStorageError<SE>
where
    SE: Debug + Display,
{
    #[error("dialogue serialization error: {0}")]
    SerdeError(SE),

    #[error("sqlite error: {0}")]
    SqliteError(#[from] sqlx::Error),

    /// Returned from [`SqliteStorage::remove_dialogue`].
    #[error("row not found")]
    DialogueNotFound,
}

impl<S> SqliteStorage<S> {
    pub async fn open(
        config: &config::Database,
        serializer: S,
    ) -> Result<Arc<Self>, SqliteStorageError<Infallible>> {
        let pool = SqlitePool::connect(format!("sqlite:{}?mode=rwc", config.path).as_str()).await?;
        let mut conn = pool.acquire().await?;
        sqlx::query(
            r#"
CREATE TABLE IF NOT EXISTS teloxide_dialogues (
    chat_id BIGINT PRIMARY KEY,
    dialogue BLOB NOT NULL
);
        "#,
        )
        .execute(&mut conn)
        .await?;

        Ok(Arc::new(Self { pool, serializer }))
    }
}

impl<S, D> Storage<D> for SqliteStorage<S>
where
    S: Send + Sync + Serializer<D> + 'static,
    D: Send + Serialize + Debug + DeserializeOwned + 'static,
    <S as Serializer<D>>::Error: Debug + Display,
{
    type Error = SqliteStorageError<<S as Serializer<D>>::Error>;

    #[instrument(skip(self))]
    /// Returns [`sqlx::Error::RowNotFound`] if a dialogue does not exist.
    fn remove_dialogue(
        self: Arc<Self>,
        ChatId(chat_id): ChatId,
    ) -> BoxFuture<'static, Result<(), Self::Error>> {
        Box::pin(async move {
            let deleted_rows_count =
                sqlx::query("DELETE FROM teloxide_dialogues WHERE chat_id = ?")
                    .bind(chat_id)
                    .execute(&self.pool)
                    .await?
                    .rows_affected();

            if deleted_rows_count == 0 {
                return Err(SqliteStorageError::DialogueNotFound);
            }

            Ok(())
        })
    }

    #[instrument(skip(self))]
    fn update_dialogue(
        self: Arc<Self>,
        ChatId(chat_id): ChatId,
        dialogue: D,
    ) -> BoxFuture<'static, Result<(), Self::Error>> {
        Box::pin(async move {
            let d = self
                .serializer
                .serialize(&dialogue)
                .map_err(SqliteStorageError::SerdeError)?;

            self.pool
                .acquire()
                .await?
                .execute(
                    sqlx::query(
                        r#"
            INSERT INTO teloxide_dialogues VALUES (?, ?)
            ON CONFLICT(chat_id) DO UPDATE SET dialogue=excluded.dialogue
                                "#,
                    )
                    .bind(chat_id)
                    .bind(d),
                )
                .await?;
            Ok(())
        })
    }

    #[instrument(skip(self))]
    fn get_dialogue(
        self: Arc<Self>,
        chat_id: ChatId,
    ) -> BoxFuture<'static, Result<Option<D>, Self::Error>> {
        trace!("Requested a dialogue #{}", chat_id);
        Box::pin(async move {
            get_dialogue(&self.pool, chat_id)
                .await?
                .map(|d| {
                    self.serializer
                        .deserialize(&d)
                        .map_err(SqliteStorageError::SerdeError)
                })
                .transpose()
        })
    }
}

impl<S> SqliteStorage<S> {
    #[instrument(skip(self))]
    pub async fn get_all_dialogues<D>(
        &self,
    ) -> Result<HashMap<ChatId, D>, SqliteStorageError<<S as Serializer<D>>::Error>>
    where
        S: Send + Sync + Serializer<D> + 'static,
        D: Send + Serialize + Debug + DeserializeOwned + 'static,
        <S as Serializer<D>>::Error: Debug + Display,
    {
        trace!("Requested all dialogues");

        #[derive(sqlx::FromRow)]
        struct DialogueDbRow {
            chat_id: i64,
            dialogue: Vec<u8>,
        }

        sqlx::query_as::<_, DialogueDbRow>("SELECT chat_id, dialogue FROM teloxide_dialogues")
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteStorageError::SqliteError)?
            .into_iter()
            .map(|row| {
                let chat_id = row.chat_id;

                let dialogue = self
                    .serializer
                    .deserialize(&row.dialogue)
                    .map_err(SqliteStorageError::SerdeError)?;

                Ok((ChatId(chat_id), dialogue))
            })
            .collect::<Result<HashMap<_, _>, _>>()
    }
}

async fn get_dialogue(
    pool: &SqlitePool,
    ChatId(chat_id): ChatId,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct DialogueDbRow {
        dialogue: Vec<u8>,
    }

    let bytes = sqlx::query_as::<_, DialogueDbRow>(
        "SELECT dialogue FROM teloxide_dialogues WHERE chat_id = ?",
    )
    .bind(chat_id)
    .fetch_optional(pool)
    .await?
    .map(|r| r.dialogue);

    Ok(bytes)
}
