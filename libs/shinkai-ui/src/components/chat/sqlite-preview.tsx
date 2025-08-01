import { debug } from '@tauri-apps/plugin-log';
import { useEffect, useState } from 'react';
import * as sqljs from 'sql.js';

import { Button } from '../button';
import { Skeleton } from '../skeleton';

interface SqlitePreviewProps {
  url: string;
}

const loadTableData = async (
  db: sqljs.Database,
  tableName: string,
): Promise<{ values: sqljs.SqlValue[][] }> => {
  void debug(`loading table data for ${tableName}`);
  const dataResult = db.exec(`SELECT * FROM ${tableName}`);
  void debug(`found ${dataResult[0].values.length} rows for ${tableName}`);
  return { values: dataResult[0].values };
};

const loadDatabase = async (
  url: string,
): Promise<{
  db: sqljs.Database;
  tables: { name: string; columns: sqljs.SqlValue[]; count: number }[];
}> => {
  void debug(`loading database from ${url}`);
  const SQL = await sqljs.default({
    locateFile: (file: string) => '/sqljs/sql-wasm.wasm',
  });

  const response = await fetch(url);
  const arrayBuffer = await response.arrayBuffer();
  const db = new SQL.Database(new Uint8Array(arrayBuffer));

  // Get list of tables
  const tablesResult = db.exec(
    "SELECT name FROM sqlite_master WHERE type='table'",
  );
  const tableNames = tablesResult[0].values.map((v) => v[0] as string);
  void debug(`found ${tableNames.length} tables`);
  void debug(`tables: ${tableNames.join(', ')}`);
  const tables = tableNames.map((table) => {
    const columnsResult = db.exec(`PRAGMA table_info(${table})`);
    const columnNames = columnsResult[0].values.map((v) => v[1]);
    const countResult = db.exec(`SELECT COUNT(*) FROM ${table}`);
    const count = countResult[0].values[0][0] as number;
    void debug(
      `found columns ${columnNames.join(', ')} and ${count} rows for ${table}`,
    );
    return {
      name: table,
      columns: columnNames,
      count,
    };
  });

  return { db, tables };
};

export const SqlitePreview: React.FC<SqlitePreviewProps> = ({ url }) => {
  const [selectedTable, setSelectedTable] = useState<string>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [loadingData, setLoadingData] = useState(false);

  const [db, setDb] = useState<sqljs.Database | null>(null);
  const [tables, setTables] = useState<
    { name: string; columns: sqljs.SqlValue[]; count: number }[]
  >([]);
  const [tableData, setTableData] = useState<{
    columns: sqljs.SqlValue[];
    rows: sqljs.SqlValue[][];
  } | null>(null);

  useEffect(() => {
    const init = async () => {
      setLoading(true);
      try {
        const { db, tables } = await loadDatabase(url);
        setDb(db);
        setTables(tables);
        if (tables.length > 0) {
          setSelectedTable(tables[0].name);
        }
      } catch (error) {
        setError(
          error instanceof Error ? error.message : 'Failed to load database',
        );
      } finally {
        setLoading(false);
      }
    };
    void init();
  }, [url]);

  useEffect(() => {
    if (db && selectedTable) {
      setLoadingData(true);
      loadTableData(db, selectedTable)
        .then(({ values }) => {
          const columns = tables.find(
            (table) => table.name === selectedTable,
          )?.columns;
          if (!columns) {
            setError('Table not found');
            return;
          }
          setTableData({
            columns,
            rows: values,
          });
        })
        .catch((error) => {
          setError(
            error instanceof Error
              ? error.message
              : 'Failed to load table data',
          );
        })
        .finally(() => {
          setLoadingData(false);
        });
    }
  }, [db, selectedTable, tables]);

  useEffect(() => {
    return () => {
      if (db) {
        void debug('closing database connection');
        db.close();
      }
    };
  }, [db]);

  if (error) {
    return (
      <div className="p-4 text-red-500">Error loading database: {error}</div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col gap-4 rounded-lg bg-gray-600 p-4">
        <div className="flex gap-2">
          <Skeleton className="h-8 w-24" />
          <Skeleton className="h-8 w-24" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 overflow-y-hidden rounded-lg bg-gray-600 p-4">
      <div className="flex flex-wrap gap-2">
        {tables.map((table) => (
          <Button
            key={table.name}
            onClick={() => setSelectedTable(table.name)}
            size="sm"
            variant={selectedTable === table.name ? 'default' : 'outline'}
          >
            {table.name} ({table.count})
          </Button>
        ))}
      </div>

      {selectedTable && (
        <div className="max-h-full min-h-fit overflow-y-auto">
          <div className="overflow-x-auto">
            {loadingData ? (
              <div className="space-y-2">
                <Skeleton className="h-8 w-full" />
                <Skeleton className="h-8 w-full" />
                <Skeleton className="h-8 w-full" />
              </div>
            ) : (
              <table className="w-full text-left text-sm">
                <thead className="sticky top-0 bg-gray-600">
                  <tr className="border-b border-gray-500">
                    {tableData?.columns.map((column) => (
                      <th
                        className="p-2 font-medium text-gray-300"
                        key={String(column)}
                      >
                        {String(column)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {tableData?.rows.map((row, i) => (
                    <tr className="border-b border-gray-700" key={i}>
                      {row.map((cell, j) => (
                        <td className="text-text-secondary p-2" key={j}>
                          {String(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
