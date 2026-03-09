use terminal_size::{Width, terminal_size};

use crate::{database::basic::SqlRead, errors::CatError};

/// database read-only
pub async fn list_pacs(conn: &mut impl SqlRead) -> Result<(), CatError> {
    let pacs = conn.get_pac_names().await?;
    if pacs.is_empty() {
        println!("No packages installed.");
    } else {
        println!("Installed packages:\n");
        print_columns_vertical(&pacs);
    }
    Ok(())
}

/// database read-only
pub async fn list_leaves(conn: &mut impl SqlRead) -> Result<(), CatError> {
    let pacs = conn.get_pacs(true).await?;
    if pacs.is_empty() {
        println!("No packages installed.");
    } else {
        println!("Installed leave packages:\n");
        print_columns_vertical(&pacs);
    }
    Ok(())
}

pub fn print_columns_vertical<S>(items: &[S])
where
    S: AsRef<str>,
{
    if items.is_empty() {
        return;
    }

    // Terminal width (default to 80 if unknown)
    let term_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    let n = items.len();

    // Try maximum possible columns and find the one that fits best
    let mut best_cols = 1;
    let mut best_widths = vec![0];

    for cols in (1..=n).rev() {
        let rows = n.div_ceil(cols);
        let mut widths = vec![0; cols];

        for col in 0..cols {
            for row in 0..rows {
                if let Some(item) = items.get(row + col * rows) {
                    widths[col] = widths[col].max(item.as_ref().len());
                }
            }
        }

        let total_width = widths.iter().sum::<usize>() + 4 * (cols - 1) + 4;
        if total_width <= term_width {
            best_cols = cols;
            best_widths = widths;
            break;
        }
    }

    let cols = best_cols;
    let rows = n.div_ceil(cols);

    // Output items top-to-bottom, left-to-right
    for row in 0..rows {
        for col in 0..cols {
            if let Some(item) = items.get(row + col * rows) {
                if col == cols - 1 {
                    print!("{}", item.as_ref());
                } else {
                    print!("{:<width$}", item.as_ref(), width = best_widths[col] + 4);
                }
            }
        }
        println!();
    }
}
