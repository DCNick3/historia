mod hack;

use std::{
    fmt::Debug,
    future::{Future, IntoFuture},
    pin::Pin,
    task::{self, Poll},
};

use crate::requester_forward;
use teloxide::{
    requests::{HasPayload, Output, Payload, Request, Requester},
    types::*,
};

use futures::ready;
use tracing::{trace_span, Span};
use url::Url;

#[derive(Clone, Debug)]
pub struct Trace<B> {
    inner: B,
    settings: Settings,
}

impl<B> Trace<B> {
    pub fn new(inner: B, settings: Settings) -> Self {
        Self { inner, settings }
    }

    pub fn inner(&self) -> &B {
        &self.inner
    }

    #[allow(unused)]
    pub fn into_inner(self) -> B {
        self.inner
    }
}

bitflags::bitflags! {
    /// [`Trace`] settings that determine what will be logged.
    ///
    /// ## Examples
    ///
    /// ```
    /// use teloxide_core::adaptors::trace::Settings;
    ///
    /// // Trace nothing
    /// let _ = Settings::empty();
    /// // Trace only requests
    /// let _ = Settings::TRACE_REQUESTS;
    /// // Trace requests verbosely and responses (non verbosely)
    /// let _ = Settings::TRACE_REQUESTS_VERBOSE | Settings::TRACE_RESPONSES;
    /// ```
    pub struct Settings: u8 {
        /// Trace requests as spans.
        ///
        /// Without [`TRACE_REQUESTS_VERBOSE`] this will only include the request type.
        const TRACE_REQUESTS = 0b00000001;

        /// Trace requests verbosely (with payloads).
        ///
        /// Implies [`TRACE_REQUESTS`]
        const TRACE_REQUESTS_VERBOSE = 0b00000011;

        /// Includes response payload in the span.
        const TRACE_RESPONSES_VERBOSE = 0b00001100;

        /// Trace everything verbosely.
        ///
        /// Implies [`TRACE_REQUESTS_VERBOSE`] and [`TRACE_RESPONSES_VERBOSE`].
        const TRACE_EVERYTHING_VERBOSE = Self::TRACE_REQUESTS_VERBOSE.bits | Self::TRACE_RESPONSES_VERBOSE.bits;
    }
}

macro_rules! fty {
    ($T:ident) => {
        TraceRequest<B::$T>
    };
}

macro_rules! fwd_inner {
    ($m:ident $this:ident ($($arg:ident : $T:ty),*)) => {
        {
            let inner = $this.inner().$m($($arg),*);
            let span = TraceRequest::make_span(&inner, $this.settings);
            TraceRequest {
                inner,
                span,
                settings: $this.settings
            }
        }
    };
}

impl<B> Requester for Trace<B>
where
    B: Requester,
{
    type Err = B::Err;

    requester_forward! {
        get_me,
        log_out,
        close,
        get_updates,
        set_webhook,
        delete_webhook,
        get_webhook_info,
        forward_message,
        copy_message,
        send_message,
        send_photo,
        send_audio,
        send_document,
        send_video,
        send_animation,
        send_voice,
        send_video_note,
        send_media_group,
        send_location,
        edit_message_live_location,
        edit_message_live_location_inline,
        stop_message_live_location,
        stop_message_live_location_inline,
        send_venue,
        send_contact,
        send_poll,
        send_dice,
        send_chat_action,
        get_user_profile_photos,
        get_file,
        kick_chat_member,
        ban_chat_member,
        unban_chat_member,
        restrict_chat_member,
        promote_chat_member,
        set_chat_administrator_custom_title,
        ban_chat_sender_chat,
        unban_chat_sender_chat,
        set_chat_permissions,
        export_chat_invite_link,
        create_chat_invite_link,
        edit_chat_invite_link,
        revoke_chat_invite_link,
        set_chat_photo,
        delete_chat_photo,
        set_chat_title,
        set_chat_description,
        pin_chat_message,
        unpin_chat_message,
        unpin_all_chat_messages,
        leave_chat,
        get_chat,
        get_chat_administrators,
        get_chat_members_count,
        get_chat_member_count,
        get_chat_member,
        set_chat_sticker_set,
        delete_chat_sticker_set,
        get_forum_topic_icon_stickers,
        create_forum_topic,
        edit_forum_topic,
        close_forum_topic,
        reopen_forum_topic,
        delete_forum_topic,
        unpin_all_forum_topic_messages,
        edit_general_forum_topic,
        close_general_forum_topic,
        reopen_general_forum_topic,
        hide_general_forum_topic,
        unhide_general_forum_topic,
        answer_callback_query,
        set_my_commands,
        get_my_commands,
        set_chat_menu_button,
        get_chat_menu_button,
        set_my_default_administrator_rights,
        get_my_default_administrator_rights,
        delete_my_commands,
        answer_inline_query,
        answer_web_app_query,
        edit_message_text,
        edit_message_text_inline,
        edit_message_caption,
        edit_message_caption_inline,
        edit_message_media,
        edit_message_media_inline,
        edit_message_reply_markup,
        edit_message_reply_markup_inline,
        stop_poll,
        delete_message,
        send_sticker,
        get_sticker_set,
        get_custom_emoji_stickers,
        upload_sticker_file,
        create_new_sticker_set,
        add_sticker_to_set,
        set_sticker_position_in_set,
        delete_sticker_from_set,
        set_sticker_set_thumb,
        send_invoice,
        create_invoice_link,
        answer_shipping_query,
        answer_pre_checkout_query,
        set_passport_data_errors,
        send_game,
        set_game_score,
        set_game_score_inline,
        get_game_high_scores,
        approve_chat_join_request,
        decline_chat_join_request
        => fwd_inner, fty
    }
}

#[must_use = "Requests are lazy and do nothing unless sent"]
pub struct TraceRequest<R> {
    inner: R,
    span: Span,
    settings: Settings,
}

impl<R> TraceRequest<R>
where
    R: Request,
{
    fn make_span(inner: &R, settings: Settings) -> Span
    where
        R::Payload: Debug,
    {
        let span = trace_span!(
            "teloxide request",
            // those will be handled by opentelemetry_tracing
            otel.kind = "client",
            otel.name = format!("teloxide/{}", <R::Payload as Payload>::NAME),
            otel.status_code = tracing::field::Empty,
            error.message = tracing::field::Empty,
            rpc.method = <R::Payload as Payload>::NAME,
            teloxide.payload = tracing::field::Empty,
            teloxide.response = tracing::field::Empty,
        );

        if settings.contains(Settings::TRACE_REQUESTS_VERBOSE) {
            span.record(
                "teloxide.payload",
                format!("{:?}", inner.payload_ref()).as_str(),
            );
            span
        } else if settings.contains(Settings::TRACE_REQUESTS) {
            span
        } else {
            Span::none()
        }
    }

    fn trace_response_fn(&self) -> fn(&Span, &Result<Output<R>, R::Err>)
    where
        Output<R>: Debug,
        R::Err: Debug,
    {
        if self.settings.contains(Settings::TRACE_RESPONSES_VERBOSE) {
            |span, response| match response.as_ref() {
                Ok(payload) => {
                    span.record("teloxide.response", format!("{:?}", payload).as_str());
                }
                Err(error) => {
                    span.record("otel.status_code", "ERROR");
                    span.record("error.message", format!("{:?}", error).as_str());
                }
            }
        } else {
            |span, response| match response.as_ref() {
                Ok(_) => {}
                Err(error) => {
                    span.record("otel.status_code", "ERROR");
                    span.record("error.message", format!("{:?}", error).as_str());
                }
            }
        }
    }
}

impl<R> HasPayload for TraceRequest<R>
where
    R: HasPayload,
{
    type Payload = R::Payload;

    fn payload_mut(&mut self) -> &mut Self::Payload {
        self.inner.payload_mut()
    }

    fn payload_ref(&self) -> &Self::Payload {
        self.inner.payload_ref()
    }
}

impl<R> Request for TraceRequest<R>
where
    R: Request,
    Output<R>: Debug,
    R::Err: Debug,
    R::Payload: Debug,
{
    type Err = R::Err;

    type Send = Send<R::Send>;

    type SendRef = Send<R::SendRef>;

    fn send(self) -> Self::Send {
        Send {
            trace_fn: self.trace_response_fn(),
            span: self.span,
            inner: self.inner.send(),
        }
    }

    fn send_ref(&self) -> Self::SendRef {
        Send {
            trace_fn: self.trace_response_fn(),
            span: self.span.clone(),
            inner: self.inner.send_ref(),
        }
    }
}

impl<R> IntoFuture for TraceRequest<R>
where
    R: Request,
    Output<R>: Debug,
    R::Err: Debug,
    R::Payload: Debug,
{
    type Output = Result<Output<Self>, <Self as Request>::Err>;
    type IntoFuture = <Self as Request>::Send;

    fn into_future(self) -> Self::IntoFuture {
        self.send()
    }
}

#[pin_project::pin_project]
pub struct Send<F>
where
    F: Future,
{
    trace_fn: fn(&Span, &F::Output),
    span: Span,
    #[pin]
    inner: F,
}

impl<F> Future for Send<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let enter = this.span.enter();
        let ret = ready!(this.inner.poll(cx));
        drop(enter);

        (this.trace_fn)(this.span, &ret);
        Poll::Ready(ret)
    }
}
