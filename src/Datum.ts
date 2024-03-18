import { addCmd } from "./commands/addCmd";
import { backupCmd } from "./commands/backupCmd";
import { deleteCmd } from "./commands/deleteCmd";
import { editCmd } from "./commands/editCmd";
import { endCmd } from "./commands/endCmd";
import { getCmd } from "./commands/getCmd";
import { grepCmd } from "./commands/grepCmd";
import { headCmd } from "./commands/headCmd";
import { mapCmd } from "./commands/mapCmd";
import { occurCmd } from "./commands/occurCmd";
import { reduceCmd } from "./commands/reduceCmd";
import { setupCmd } from "./commands/setupCmd";
import { startCmd } from "./commands/startCmd";
import { switchCmd } from "./commands/switchCmd";
import { tailCmd } from "./commands/tailCmd";
import { updateCmd } from "./commands/updateCmd";
import { v1Cmd } from "./commands/v1Cmd";
import { DbArgs } from "./input/dbArgs";

export class Datum {
  // Properties
  dbArgs: DbArgs;
  // Methods
  // Constructors
  constructor(dbArgs?: DbArgs) {
    this.dbArgs = dbArgs ?? {};
  }

  add: (args: Parameters<typeof addCmd>[0]) => ReturnType<typeof addCmd> = (
    args,
  ) => {
    return addCmd(args, this.dbArgs);
  };

  backup: (
    args: Parameters<typeof backupCmd>[0],
  ) => ReturnType<typeof backupCmd> = (args) => {
    return backupCmd(args, this.dbArgs);
  };

  delete: (
    args: Parameters<typeof deleteCmd>[0],
  ) => ReturnType<typeof deleteCmd> = (args) => {
    return deleteCmd(args, this.dbArgs);
  };

  edit: (args: Parameters<typeof editCmd>[0]) => ReturnType<typeof editCmd> = (
    args,
  ) => {
    return editCmd(args, this.dbArgs);
  };

  end: (args: Parameters<typeof endCmd>[0]) => ReturnType<typeof endCmd> = (
    args,
  ) => {
    return endCmd(args, this.dbArgs);
  };

  get: (args: Parameters<typeof getCmd>[0]) => ReturnType<typeof getCmd> = (
    args,
  ) => {
    return getCmd(args, this.dbArgs);
  };

  grep: (args: Parameters<typeof grepCmd>[0]) => ReturnType<typeof grepCmd> = (
    args,
  ) => {
    return grepCmd(args, this.dbArgs);
  };

  head: (args: Parameters<typeof headCmd>[0]) => ReturnType<typeof headCmd> = (
    args,
  ) => {
    return headCmd(args, this.dbArgs);
  };

  map: (args: Parameters<typeof mapCmd>[0]) => ReturnType<typeof mapCmd> = (
    args,
  ) => {
    return mapCmd(args, this.dbArgs);
  };

  occur: (args: Parameters<typeof occurCmd>[0]) => ReturnType<typeof occurCmd> =
    (args) => {
      return occurCmd(args, this.dbArgs);
    };

  reduce: (
    args: Parameters<typeof reduceCmd>[0],
  ) => ReturnType<typeof reduceCmd> = (args) => {
    return reduceCmd(args, this.dbArgs);
  };

  setup: (args: Parameters<typeof setupCmd>[0]) => ReturnType<typeof setupCmd> =
    (args) => {
      return setupCmd(args, this.dbArgs);
    };

  start: (args: Parameters<typeof startCmd>[0]) => ReturnType<typeof startCmd> =
    (args) => {
      return startCmd(args, this.dbArgs);
    };

  switch: (
    args: Parameters<typeof switchCmd>[0],
  ) => ReturnType<typeof switchCmd> = (args) => {
    return switchCmd(args, this.dbArgs);
  };

  tail: (args: Parameters<typeof tailCmd>[0]) => ReturnType<typeof tailCmd> = (
    args,
  ) => {
    return tailCmd(args, this.dbArgs);
  };

  update: (
    args: Parameters<typeof updateCmd>[0],
  ) => ReturnType<typeof updateCmd> = (args) => {
    return updateCmd(args, { ...this.dbArgs, strategy: "preferNew" });
  };

  merge: (
    args: Parameters<typeof updateCmd>[0],
  ) => ReturnType<typeof updateCmd> = (args) => {
    return updateCmd(args, { ...this.dbArgs, strategy: "merge" });
  };

  v1: (args: Parameters<typeof v1Cmd>[0]) => ReturnType<typeof v1Cmd> = (
    args,
  ) => {
    return v1Cmd(args, this.dbArgs);
  };
}
