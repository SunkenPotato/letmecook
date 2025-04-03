-- Add migration script here
alter table recipes
alter column author
set
    not null;

alter table recipes
alter column name
set
    not null;
