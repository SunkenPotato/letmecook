-- Add migration script here

alter table recipes add author integer references users(id), add views integer default 0;
