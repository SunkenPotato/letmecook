-- Add migration script here
alter table recipes
alter column description
set
    not null;
