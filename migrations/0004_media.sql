-- A photo or video posted with the message: a link to the file on
-- YourImageShare, which the page uploaded it to. Champt never holds the file
-- and never deletes it; its uploader can. Null for a message without one.
ALTER TABLE messages ADD COLUMN media TEXT;
