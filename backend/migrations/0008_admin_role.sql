-- The admin role (ADR 0005): demo and test databases seed an administrator
-- account that reaches the admin routes through its bearer session, next to
-- the configured shared admin token.
ALTER TABLE accounts DROP CONSTRAINT accounts_role_check;
ALTER TABLE accounts ADD CHECK (role IN ('member','creator','admin'));
