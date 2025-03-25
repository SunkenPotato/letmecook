-- Add migration script here
create table recipes (
    id serial primary key,
    name varchar,
    author int references users (id),
    deleted bool not null default false,
    createdAt timestamp not null default now ()
);
