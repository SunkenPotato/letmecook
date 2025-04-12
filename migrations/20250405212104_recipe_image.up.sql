-- Add up migration script here
alter table recipes
add column image text not null;
