# draft-emails: working with mail through the tools

The five mounted tools are the whole mailbox surface: list_folders,
list_messages, read_message, save_draft, discard_draft. Each call runs
astra-cli behind the scenes with the turn's correlation id, so every read
and every draft is audited. This guide may lag; the tool descriptions are
authoritative.

## Reading

1. list_folders with an account id to see the folder tree (id, name, role,
   parent, unread count).
2. list_messages on a folder (or the account window) to page the list:
   key, from, subject, date, unread, draft flag, attachment flag, preview.
3. read_message with one or more keys for the envelope and the plain text
   body. The HTML alternative is only reported present; read the plain form.

Account ids come from the host context or the user; the CLI reads them from
the account index and refuses an unknown id with a runtime error.

## Drafting

1. Compose the reply, then call save_draft with from, to (a list), cc, the
   subject, and the body text. The user sees an approval card before
   anything is written; that card is the decision, so never ask separately
   first.
2. discard_draft removes a stored draft the same way, with its own card.
3. A second save of one composition stores a second draft beside the first.
   If the user refines a draft, say so and discard the old one.

## Output conventions

Every CLI answer is one JSON envelope: `{"schema":1,"ok":true,"data":...}`
or `{"schema":1,"ok":false,"error":{"code":"usage"|"runtime"|"refused",
"message":"..."}}`. Exit codes 0 ok, 1 runtime, 2 usage, 3 refused. A tool
result carries the envelope verbatim; read the error message before
retrying, and never retry a usage error with the same arguments.

## Safety boundary

Sending is a human action: the app sends, the CLI refuses the send class,
and no tool here can send. Draft, let the user approve, and let the user
send from the app. Archive and label verbs arrive with their slices; until
then, tell the user those actions are not available rather than working
around them.
