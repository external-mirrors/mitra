# Markup

Mitra supports a subset of [CommonMark](http://commonmark.org/) spec:

- **Bold**, *italic*
- `inline code` and code blocks
- Links
- Headings (level 1 only)

And the following extensions and microsyntaxes:

- [GFM](https://github.github.com/gfm/) autolink extension (only the following URI schemes: `http:`, `https:`, `mailto:`, `xmpp:`, `gemini:`).
- GFM ~~strikethrough~~: `~~strikethrough~~`.
- [DFM](https://support.discord.com/hc/en-us/articles/210298617-Markdown-Text-101-Chat-Formatting-Bold-Italic-Underline) underline extension: `__underline__`.
- Hashtags: `#tag`.
- Mentions: `@user@server.example`. For local users the server part can be omitted: `@user`.
- References to other posts: `[[post-id]]` and `[[post-id|link-text]]` (where `post-id` is an ID of ActivityPub object).
- Emoji shortcodes: `:emojiname:`.
- Greentext: `>greentext` (only in mitra-web client).
