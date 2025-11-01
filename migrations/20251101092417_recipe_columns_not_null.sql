-- Add migration script here

alter table recipes alter column author set not null;
alter table recipes alter column views set not null;
alter table recipes alter column steps set not null;
